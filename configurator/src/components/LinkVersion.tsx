import S, { type DataSignal } from "@surplus/s";
import { type Device } from "../lib/device";

export default ({ device }: { device: Device }) => {
	const versionMajor = S.value(undefined as number | "ERR" | undefined);
	const versionMinor = S.value(undefined as number | "ERR" | undefined);
	const versionPatch = S.value(undefined as number | "ERR" | undefined);

	device.request("GetVersionMajor").then((response) => {
		if (typeof response === "object" && "Uint" in response) {
			versionMajor(response.Uint);
		} else {
			console.error("Unexpected response for GetVersionMajor:", response);
			versionMajor("ERR");
		}
	});

	device.request("GetVersionMinor").then((response) => {
		if (typeof response === "object" && "Uint" in response) {
			versionMinor(response.Uint);
		} else {
			console.error("Unexpected response for GetVersionMinor:", response);
			versionMinor("ERR");
		}
	});

	device.request("GetVersionPatch").then((response) => {
		if (typeof response === "object" && "Uint" in response) {
			versionPatch(response.Uint);
		} else {
			console.error("Unexpected response for GetVersionPatch:", response);
			versionPatch("ERR");
		}
	});

	return (
		<span>
			{versionMajor()}.{versionMinor()}.{versionPatch()}
		</span>
	);
};
