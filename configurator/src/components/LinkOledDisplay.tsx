import S from "@surplus/s";
import { Surplus } from "@surplus/types";
import { type Device } from "../lib/device";

export default ({ device }) => (
	<canvas width={256} height={64} fn={(c) => startCanvas(c, device)} />
);

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function startCanvas(
	canvas: Surplus.Element<HTMLCanvasElement>,
	device: Device,
) {
	const ctx = canvas.getContext("2d");
	if (!ctx) {
		throw new Error("Failed to get canvas context");
	}

	let lastFrame = -1;
	const interval = setInterval(async () => {
		const newFrame = await device.request("GetFrameCount");
		if (
			!(typeof newFrame === "object" && "Uint" in newFrame) ||
			newFrame.Uint === lastFrame
		) {
			return;
		}
		lastFrame = newFrame.Uint;

		// Get the pixel data. It's a 256x64 grid of 4 bit grayscale, encoded into a 128x64 grid of bytes (each byte contains two pixels).
		const pixelData = (await device.request("GetFrame")) as Uint8Array;
		const imageData = ctx.createImageData(256, 64);

		for (let i = 0; i < pixelData.length; i++) {
			const byte = pixelData[i];
			const highNibble = byte >> 4;
			const lowNibble = byte & 0x0f;

			const highPixelValue = (highNibble / 0x0f) * 255;
			const lowPixelValue = (lowNibble / 0x0f) * 255;

			const pixelIndex1 = i * 2;
			const pixelIndex2 = i * 2 + 1;

			imageData.data[pixelIndex1 * 4] = highPixelValue;
			imageData.data[pixelIndex1 * 4 + 1] = highPixelValue;
			imageData.data[pixelIndex1 * 4 + 2] = highPixelValue;
			imageData.data[pixelIndex1 * 4 + 3] = highPixelValue > 0 ? 255 : 0;

			imageData.data[pixelIndex2 * 4] = lowPixelValue;
			imageData.data[pixelIndex2 * 4 + 1] = lowPixelValue;
			imageData.data[pixelIndex2 * 4 + 2] = lowPixelValue;
			imageData.data[pixelIndex2 * 4 + 3] = lowPixelValue > 0 ? 255 : 0;
		}

		ctx.putImageData(imageData, 0, 0);
	}, 1000 / 10);

	S.cleanup(() => clearInterval(interval));
}
