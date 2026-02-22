import S, { type DataSignal } from "@surplus/s";
import { Response } from "../../wasm/linkproto";

// @ts-ignore
import * as C from "./LinkLights.css";

import LinkLight from "./LinkLight";
import { type Device } from "../lib/device";

type LightState = Extract<Response, { LightState: unknown }>["LightState"];

export default ({ device }: { device: Device }) => {
	const deviceState: DataSignal<LightState> = S.data({
		debug_leds: [0, 0, 0],
		debug_leds_max_duty: 1,
		led_controller: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
	});

	const interval = setInterval(() => {
		device.request("GetLightState").then((response) => {
			if (typeof response === "object" && "LightState" in response) {
				deviceState(response.LightState);
			}
		});
	}, 1000 / 10);

	S.cleanup(() => clearInterval(interval));

	const debugLed1 = S(() =>
		Math.round(
			(deviceState().debug_leds[0] / deviceState().debug_leds_max_duty) *
				255.0,
		),
	);
	const debugLed2 = S(() =>
		Math.round(
			(deviceState().debug_leds[1] / deviceState().debug_leds_max_duty) *
				255.0,
		),
	);
	const debugLed3 = S(() =>
		Math.round(
			(deviceState().debug_leds[2] / deviceState().debug_leds_max_duty) *
				255.0,
		),
	);

	const channels: any[] = [];
	for (let i = 0; i < 18; i++) {
		((i) => {
			channels.push(
				S(() => ((deviceState().led_controller[i] | 0) >> 24) & 0xff),
			);
			channels.push(
				S(() => ((deviceState().led_controller[i] | 0) >> 16) & 0xff),
			);
			channels.push(
				S(() => ((deviceState().led_controller[i] | 0) >> 8) & 0xff),
			);
			channels.push(
				S(() => (deviceState().led_controller[i] | 0) & 0xff),
			);
		})(i);
	}

	return (
		<div>
			<LinkLight
				x="87.3mm"
				y="21mm"
				r={debugLed1}
				g={debugLed1}
				b={debugLed1}
			/>
			<LinkLight
				x="85mm"
				y="18mm"
				r={debugLed2}
				g={debugLed2}
				b={debugLed2}
			/>
			<LinkLight
				x="89mm"
				y="18mm"
				r={debugLed3}
				g={debugLed3}
				b={debugLed3}
			/>
		</div>
	);
};
