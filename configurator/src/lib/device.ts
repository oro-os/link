import S, { type DataSignal } from "@surplus/s";
import {
	Decoder,
	buffer_size,
	encode_request,
	type Request,
	type Response,
} from "link-protocol";
import initLinkProtocol from "link-protocol";
import pLimit from "p-limit";

type LinkProtocol = Awaited<ReturnType<typeof initLinkProtocol>>;

let linkProtocol: LinkProtocol | undefined;

const limit = pLimit(1);
const RESET_GARBAGE_LEN = 128;
const RESET_SENTINEL_LEN = 128;
const READ_TIMEOUT_MS = 1000;
const RESET_BURN_READS = 256;

class RecoverableTransportError extends Error {
	constructor(message: string, cause?: unknown) {
		super(message);
		this.name = "RecoverableTransportError";
		if (cause !== undefined) {
			(this as Error & { cause?: unknown }).cause = cause;
		}
	}
}

export class Device {
	public readonly online: DataSignal<boolean> = S.value(false);
	#port: SerialPort | null;
	#reader: ReadableStreamDefaultReader<Uint8Array> | null;
	#writer: WritableStreamDefaultWriter<Uint8Array> | null;
	#decoder: Decoder | null;
	#encodeBuffer: Uint8Array | null;
	#pendingRx: Uint8Array;

	constructor() {
		this.#port = null;
		this.#reader = null;
		this.#writer = null;
		this.#decoder = null;
		this.#encodeBuffer = null;
		this.#pendingRx = new Uint8Array(0);
	}

	async #ensureProtocol(): Promise<void> {
		if (!linkProtocol) {
			linkProtocol = await initLinkProtocol();
		}
	}

	#assertOpen() {
		if (!this.#port || !this.#reader || !this.#writer) {
			throw new Error("Device is not open");
		}
	}

	#prependPending(data: Uint8Array) {
		if (data.length === 0) {
			return;
		}

		if (this.#pendingRx.length === 0) {
			this.#pendingRx = data.slice();
			return;
		}

		const merged = new Uint8Array(data.length + this.#pendingRx.length);
		merged.set(data, 0);
		merged.set(this.#pendingRx, data.length);
		this.#pendingRx = merged;
	}

	async #readFromSerial(timeoutMs = READ_TIMEOUT_MS): Promise<Uint8Array> {
		this.#assertOpen();

		if (this.#pendingRx.length > 0) {
			const out = this.#pendingRx;
			this.#pendingRx = new Uint8Array(0);
			console.debug("[transport] consume pending bytes", {
				len: out.length,
			});
			return out;
		}

		let timer: ReturnType<typeof setTimeout> | null = null;
		try {
			const result = await Promise.race([
				this.#reader!.read(),
				new Promise<never>((_, reject) => {
					timer = setTimeout(() => {
						reject(
							new RecoverableTransportError(
								"Serial read timed out",
							),
						);
					}, timeoutMs);
				}),
			]);

			if (result.done) {
				throw new Error("Serial stream ended");
			}

			console.debug("[transport] serial read", {
				len: result.value.length,
			});
			return result.value;
		} finally {
			if (timer) {
				clearTimeout(timer);
			}
		}
	}

	async #readRawExact(len: number): Promise<Uint8Array> {
		const out = new Uint8Array(len);
		let offset = 0;

		if (this.#pendingRx.length > 0) {
			const take = Math.min(this.#pendingRx.length, len);
			out.set(this.#pendingRx.subarray(0, take), 0);
			offset = take;
			this.#pendingRx = this.#pendingRx.subarray(take);
		}

		while (offset < len) {
			const chunk = await this.#readFromSerial();
			const take = Math.min(chunk.length, len - offset);
			out.set(chunk.subarray(0, take), offset);
			offset += take;
			if (take < chunk.length) {
				this.#prependPending(chunk.subarray(take));
			}
		}

		return out;
	}

	#resetDecoderState() {
		if (!this.#decoder) {
			return;
		}

		try {
			this.#decoder.feed(new Uint8Array([0]));
		} catch {
			// Expected; this is intentionally used to abort/reset decoder stream state.
		}
	}

	async #recoverStream(reason: string): Promise<void> {
		this.#assertOpen();
		console.debug("[transport] recover stream start", { reason });

		const garbage = new Uint8Array(RESET_GARBAGE_LEN).fill(0xff);
		const sentinels = new Uint8Array(RESET_SENTINEL_LEN);

		await this.#writer!.write(garbage);
		await this.#writer!.write(sentinels);
		console.debug("[transport] wrote reset burst", {
			garbage: RESET_GARBAGE_LEN,
			sentinels: RESET_SENTINEL_LEN,
		});

		let sentinelCount = 0;
		for (let attempt = 0; attempt < RESET_BURN_READS; attempt += 1) {
			const chunk = await this.#readFromSerial();
			for (let index = 0; index < chunk.length; index += 1) {
				if (chunk[index] === 0) {
					sentinelCount += 1;
					if (sentinelCount === RESET_SENTINEL_LEN) {
						const leftoverStart = index + 1;
						if (leftoverStart < chunk.length) {
							this.#prependPending(chunk.subarray(leftoverStart));
						}
						this.#resetDecoderState();
						console.debug("[transport] recover stream complete", {
							leftover: Math.max(chunk.length - leftoverStart, 0),
						});
						return;
					}
				} else {
					sentinelCount = 0;
				}
			}
		}

		throw new RecoverableTransportError("Failed to recover stream");
	}

	async #sendRequestFrame(request: Request): Promise<void> {
		await this.#ensureProtocol();
		this.#assertOpen();

		if (!this.#encodeBuffer) {
			this.#encodeBuffer = new Uint8Array(buffer_size());
		}

		const offlen = encode_request(request, this.#encodeBuffer);
		try {
			const frame = this.#encodeBuffer.subarray(
				offlen.offset,
				offlen.offset + offlen.len,
			);
			console.debug("[transport] tx request frame", {
				len: frame.length,
				offset: offlen.offset,
			});
			await this.#writer!.write(frame);
		} finally {
			offlen.free();
		}
	}

	async #readDecodedResponse(): Promise<Response> {
		if (!this.#decoder) {
			throw new Error("Decoder is not initialized");
		}

		for (;;) {
			const incoming = await this.#readFromSerial();
			if (incoming.length === 0) {
				console.debug("[transport] decoder feed skipped empty chunk");
				continue;
			}

			let report;
			try {
				report = this.#decoder.feed(incoming);
			} catch (error) {
				console.debug("[transport] decoder feed error", error);
				throw new RecoverableTransportError(
					"Decoder feed failed",
					error,
				);
			}

			if (!report) {
				console.debug("[transport] decoder incomplete", {
					chunkLen: incoming.length,
				});
				continue;
			}

			try {
				console.debug("[transport] decoder complete", {
					decoded: report.decoded_size,
					leftover: report.leftover,
				});
				if (report.leftover > 0) {
					const start = incoming.length - report.leftover;
					this.#prependPending(incoming.subarray(start));
				}

				return this.#decoder.decode_response();
			} catch (error) {
				console.debug("[transport] decode_response error", error);
				throw new RecoverableTransportError(
					"Response decode failed",
					error,
				);
			} finally {
				report.free();
			}
		}
	}

	public async open(): Promise<void> {
		await this.close();
		await this.#ensureProtocol();

		this.#port = await navigator.serial.requestPort();

		await this.#port.open({
			baudRate: 3000000,
			bufferSize: 65536,
		});

		this.#reader = this.#port.readable?.getReader() || null;
		this.#writer = this.#port.writable?.getWriter() || null;

		if (!this.#reader || !this.#writer) {
			throw new Error("Failed to get reader or writer");
		}

		this.#decoder = Decoder.new_with_global_buffer();
		this.#encodeBuffer = new Uint8Array(buffer_size());
		this.#pendingRx = new Uint8Array(0);

		await this.#recoverStream("open");

		this.online(true);
	}

	public async request(request: Request): Promise<Response | Uint8Array> {
		this.#assertOpen();

		return await limit(async () => {
			let requestSent = false;
			for (;;) {
				try {
					if (!requestSent) {
						console.debug("[transport] request sending", request);
						await this.#sendRequestFrame(request);
						requestSent = true;
						console.debug("[transport] request sent");
					}

					const response = await this.#readDecodedResponse();
					console.debug("[transport] response decoded", response);

					if (
						typeof response === "object" &&
						"BulkTransfer" in response
					) {
						const len = response.BulkTransfer;
						console.debug("[transport] bulk transfer begin", {
							len,
						});
						const data = await this.#readRawExact(len);
						console.debug("[transport] bulk transfer complete", {
							len: data.length,
						});
						return data;
					}

					return response;
				} catch (error) {
					if (!(error instanceof RecoverableTransportError)) {
						throw error;
					}

					console.debug("[transport] recoverable error", {
						requestSent,
						error,
					});
					await this.#recoverStream(
						requestSent
							? "while waiting for response; resyncing and resending"
							: "while sending request; retry send",
					);
					// After any stream recovery both sides reset — must always resend.
					requestSent = false;
					console.debug(
						"[transport] stream recovered; retrying request send",
					);
				}
			}
		});
	}

	public async close(): Promise<void> {
		this.#decoder?.free();
		this.#reader?.releaseLock();
		this.#writer?.releaseLock();
		await this.#port?.close();
		this.#port = null;
		this.#reader = null;
		this.#writer = null;
		this.#decoder = null;
		this.#encodeBuffer = null;
		this.#pendingRx = new Uint8Array(0);
		this.online(false);
	}
}
