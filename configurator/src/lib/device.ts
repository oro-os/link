import S, { type DataSignal } from "@surplus/s";
import {
	type Request,
	type Response,
	encode_request,
	decode_response,
} from "link-protocol";
import initLinkProtocol from "link-protocol";
import pLimit from "p-limit";

type LinkProtocol = Awaited<ReturnType<typeof initLinkProtocol>>;

let linkProtocol: LinkProtocol | undefined;

const limit = pLimit(1);
const MAX_FRAME_LEN = 4096;
const RESET_REQ_WORD = 0xffffffff;
const RESET_ACK_WORD = 0xfffffffe;

class RecoverableTransportError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "RecoverableTransportError";
	}
}

function crc32(data: Uint8Array): number {
	let crc = 0xffffffff;
	for (let index = 0; index < data.length; index += 1) {
		crc ^= data[index];
		for (let bit = 0; bit < 8; bit += 1) {
			const mask = -(crc & 1);
			crc = (crc >>> 1) ^ (0xedb88320 & mask);
		}
	}
	return ~crc >>> 0;
}

function wordToBytes(value: number): Uint8Array {
	const bytes = new Uint8Array(4);
	new DataView(bytes.buffer).setUint32(0, value >>> 0, false);
	return bytes;
}

function bytesToWord(bytes: Uint8Array): number {
	return new DataView(
		bytes.buffer,
		bytes.byteOffset,
		bytes.byteLength,
	).getUint32(0, false);
}

async function writeWord(
	writer: WritableStreamDefaultWriter<Uint8Array>,
	word: number,
) {
	await writer.write(wordToBytes(word));
}

class PutBackReader {
	#reader: ReadableStreamDefaultReader<Uint8Array>;
	#buffer: Uint8Array[] = [];

	constructor(reader: ReadableStreamDefaultReader<Uint8Array>) {
		this.#reader = reader;
	}

	/**
	 * Reads the next `len` bytes from the stream. If there are not enough
	 * bytes in the internal buffer, it will read from the underlying reader until
	 * it has enough bytes to fulfill the request. Any remaining bytes are stored
	 * in the internal buffer for future reads.
	 * @param len The number of bytes to read.
	 * @returns A promise that resolves to a Uint8Array containing the read bytes.
	 */
	async read(len: number): Promise<Uint8Array> {
		while (this.#buffer.reduce((acc, buf) => acc + buf.length, 0) < len) {
			const { value, done } = await this.#reader.read();
			if (done) {
				throw new Error("Unexpected end of stream");
			}
			this.#buffer.push(value);
		}

		const result = new Uint8Array(len);
		let offset = 0;

		while (offset < len) {
			const buf = this.#buffer[0];
			const toCopy = Math.min(buf.length, len - offset);
			result.set(buf.subarray(0, toCopy), offset);
			offset += toCopy;

			if (toCopy < buf.length) {
				this.#buffer[0] = buf.subarray(toCopy);
			} else {
				this.#buffer.shift();
			}
		}

		return result;
	}

	putBack(data: Uint8Array) {
		this.#buffer.unshift(data);
	}

	clear() {
		this.#buffer = [];
	}

	take(): ReadableStreamDefaultReader<Uint8Array> {
		return this.#reader;
	}
}

async function reqres(
	reader: PutBackReader,
	writer: WritableStreamDefaultWriter<Uint8Array>,
	request: Request,
): Promise<Response | Uint8Array> {
	if (!linkProtocol) {
		linkProtocol = await initLinkProtocol();
	}

	const encodedRequest = encode_request(request);
	const requestLength = bytesToWord(encodedRequest.subarray(0, 4));
	if (requestLength === 0 || requestLength > MAX_FRAME_LEN) {
		throw new Error("Encoded request is invalid");
	}
	const requestPayload = encodedRequest.subarray(4, 4 + requestLength);
	await writer.write(encodedRequest);
	await writeWord(writer, crc32(requestPayload));

	const length = bytesToWord(await reader.read(4));
	if (length === RESET_REQ_WORD) {
		await writeWord(writer, RESET_ACK_WORD);
		throw new RecoverableTransportError("Received reset request");
	}
	if (length === RESET_ACK_WORD) {
		throw new RecoverableTransportError("Received reset acknowledgement");
	}
	if (length === 0 || length > MAX_FRAME_LEN) {
		throw new RecoverableTransportError("Received invalid response length");
	}

	const responseBytes = await reader.read(length);
	const responseCrc = bytesToWord(await reader.read(4));
	if (responseCrc !== crc32(responseBytes)) {
		throw new RecoverableTransportError("Response CRC mismatch");
	}
	const response = decode_response(responseBytes);

	if (typeof response === "object" && "BulkTransfer" in response) {
		return await reader.read(response.BulkTransfer);
	}

	return response;
}

async function recoverLink(
	reader: PutBackReader,
	writer: WritableStreamDefaultWriter<Uint8Array>,
): Promise<void> {
	reader.clear();
	await writeWord(writer, RESET_REQ_WORD);

	let window = 0;
	let filled = 0;
	for (;;) {
		const byte = (await reader.read(1))[0];
		window = ((window << 8) | byte) >>> 0;
		if (filled < 3) {
			filled += 1;
			continue;
		}

		if (window === RESET_REQ_WORD) {
			await writeWord(writer, RESET_ACK_WORD);
			continue;
		}

		if (window === RESET_ACK_WORD) {
			reader.clear();
			return;
		}
	}
}

export class Device {
	public readonly online: DataSignal<boolean> = S.value(false);
	#port: SerialPort | null;
	#reader: PutBackReader | null = null;
	#writer: WritableStreamDefaultWriter<Uint8Array> | null = null;

	constructor() {
		this.#port = null;
		this.#reader = null;
		this.#writer = null;
	}

	public async open(): Promise<void> {
		await this.close();

		this.#port = await navigator.serial.requestPort();

		await this.#port.open({
			baudRate: 3000000,
			bufferSize: 65536,
		});

		const reader = this.#port.readable?.getReader() || null;
		this.#reader = reader ? new PutBackReader(reader) : null;
		this.#writer = this.#port.writable?.getWriter() || null;

		if (!this.#reader || !this.#writer) {
			throw new Error("Failed to get reader or writer");
		}

		this.online(true);
	}

	public async request(request: Request): Promise<Response | Uint8Array> {
		if (!this.#port || !this.#reader || !this.#writer) {
			throw new Error("Device is not open");
		}

		return await limit(async () => {
			for (;;) {
				try {
					return await reqres(this.#reader!, this.#writer!, request);
				} catch (error) {
					if (!(error instanceof RecoverableTransportError)) {
						throw error;
					}

					await recoverLink(this.#reader!, this.#writer!);
				}
			}
		});
	}

	public async close(): Promise<void> {
		this.#reader?.take().releaseLock();
		this.#writer?.releaseLock();
		await this.#port?.close();
		this.#port = null;
		this.#reader = null;
		this.#writer = null;
		this.online(false);
	}
}
