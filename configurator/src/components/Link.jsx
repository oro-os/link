import * as C from "./Link.css";

import LinkOledDisplay from "./LinkOledDisplay";
import LinkLights from "./LinkLights";

export default ({ device }) => {
	return (
		<div class={C.root}>
			<div class={C.diagram}>
				<img src="diagram.svg" />
			</div>
			<div class={C.oled}>
				<div class={C.oledFrame}>
					<LinkOledDisplay device={device} />
				</div>
			</div>
			<div class={C.lights}>
				<LinkLights device={device} />
			</div>
		</div>
	);
};
