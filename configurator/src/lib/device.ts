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
			console.debug("read link bytes:", value);
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

	take(): ReadableStreamDefaultReader<Uint8Array> {
		return this.#reader;
	}
}

async function reqres(
	reader: PutBackReader,
	writer: WritableStreamDefaultWriter<Uint8Array>,
	request: Request,
): Promise<Response> {
	if (!linkProtocol) {
		linkProtocol = await initLinkProtocol();
	}

	console.debug("sending link request:", request);
	const encodedRequest = encode_request(request);
	console.debug("encoded link request bytes:", encodedRequest);
	await writer.write(encodedRequest);

	const lengthBytes = await reader.read(4);
	console.debug("read link length prefix bytes:", lengthBytes);
	const length = new DataView(lengthBytes.buffer).getUint32(0, false);
	console.debug("decoded link length prefix:", length);
	const responseBytes = await reader.read(length);
	console.debug("read link response bytes:", responseBytes);
	const response = decode_response(responseBytes);
	console.debug("decoded link response:", response);

	return response;
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

		await this.#port.open({ baudRate: 115200 });

		const reader = this.#port.readable?.getReader() || null;
		this.#reader = reader ? new PutBackReader(reader) : null;
		this.#writer = this.#port.writable?.getWriter() || null;

		if (!this.#reader || !this.#writer) {
			throw new Error("Failed to get reader or writer");
		}

		this.online(true);
	}

	public async request(request: Request): Promise<Response> {
		if (!this.#port || !this.#reader || !this.#writer) {
			throw new Error("Device is not open");
		}

		return await limit(reqres, this.#reader!, this.#writer!, request);
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
