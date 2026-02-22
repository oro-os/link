import S, { type DataSignal } from "@surplus/s";
import { Response, Request } from "../../wasm/linkproto";

// @ts-ignore
import * as C from "./LinkLights.css";

import LinkLight from "./LinkLight";
import { type Device } from "../lib/device";

type LightState = Extract<Response, { LightState: unknown }>["LightState"];
type LightProgram = Extract<
	Request,
	{ StartLightProgram: unknown }
>["StartLightProgram"];

export default ({ device }: { device: Device }) => {
	const deviceState: DataSignal<LightState> = S.data({
		debug_leds: [0, 0, 0],
		debug_leds_max_duty: 1, // prevent a divide-by-zero before we get the first state update
		controller: [0, 0, 0, 0, 0, 0, 0, 0, 0],
	});

	let timeout: undefined | number = undefined;
	const triggerUpdate = () => {
		timeout = setTimeout(() => {
			device.request("GetLightState").then((response) => {
				if (typeof response === "object" && "LightState" in response) {
					deviceState(response.LightState);
				}
				triggerUpdate();
			});
		}, 1000 / 60);
	};

	triggerUpdate();

	S.cleanup(() => clearTimeout(timeout));

	// NOTE: These lights are duty-cycle based, not brightness based,
	//       so we apply a super strong curve to make them match what's
	//       being seen physically as their brightness is very much
	//       NOT linear. The LED controller doesn't have this problem.
	const debugLed1 = S(() =>
		Math.round(
			Math.pow(
				deviceState().debug_leds[0] / deviceState().debug_leds_max_duty,
				0.1,
			) * 255.0,
		),
	);
	const debugLed2 = S(() =>
		Math.round(
			Math.pow(
				deviceState().debug_leds[1] / deviceState().debug_leds_max_duty,
				0.1,
			) * 255.0,
		),
	);
	const debugLed3 = S(() =>
		Math.round(
			Math.pow(
				deviceState().debug_leds[2] / deviceState().debug_leds_max_duty,
				0.1,
			) * 255.0,
		),
	);

	const channels: any[] = [];
	for (let i = 0; i < 18; i++) {
		((i) => {
			channels.push(
				S(() => ((deviceState().controller[i] | 0) >> 24) & 0xff),
			);
			channels.push(
				S(() => ((deviceState().controller[i] | 0) >> 16) & 0xff),
			);
			channels.push(
				S(() => ((deviceState().controller[i] | 0) >> 8) & 0xff),
			);
			channels.push(S(() => (deviceState().controller[i] | 0) & 0xff));
		})(i);
	}

	const prgDebug1 = S.data<boolean>(false);
	const prgDebug2 = S.data<boolean>(false);
	const prgDebug3 = S.data<boolean>(false);
	const prgChannels = new Array(36).fill(0).map(() => S.data<number>(0));

	const prgState = S(() => {
		let anyState = prgDebug1() || prgDebug2() || prgDebug3();
		const state: LightProgram = {
			debug: [prgDebug1(), prgDebug2(), prgDebug3()],
			controller: [0, 0, 0, 0, 0, 0, 0, 0, 0],
		};

		for (let i = 0; i < 36; i += 4) {
			const v =
				(prgChannels[i]() << 24) |
				(prgChannels[i + 1]() << 16) |
				(prgChannels[i + 2]() << 8) |
				prgChannels[i + 3]();
			state.controller[i / 4] = v;
			if (v !== 0) {
				anyState = true;
			}
		}

		return anyState ? state : null;
	});

	S(() => {
		if (prgState()) {
			device.request({ StartLightProgram: prgState()! });
		} else {
			device.request("EndLightProgram");
		}
	});

	return (
		<div>
			<LinkLight
				x="87.3mm"
				y="21mm"
				r={debugLed1}
				g={debugLed1}
				b={debugLed1}
				onStartDebug={() => prgDebug1(true)}
				onEndDebug={() => prgDebug1(false)}
			/>
			<LinkLight
				x="85mm"
				y="18mm"
				r={debugLed2}
				g={debugLed2}
				b={debugLed2}
				onStartDebug={() => prgDebug2(true)}
				onEndDebug={() => prgDebug2(false)}
			/>
			<LinkLight
				x="89mm"
				y="18mm"
				r={debugLed3}
				g={debugLed3}
				b={debugLed3}
				onStartDebug={() => prgDebug3(true)}
				onEndDebug={() => prgDebug3(false)}
			/>
		</div>
	);
};
