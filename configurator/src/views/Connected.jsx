import * as C from "./Connected.css";

import SvgLinkUsbDiagram from "../components/SvgLinkUsbDiagram";
import LinkVersion from "../components/LinkVersion";

export default ({ device }) => (
	<div class={C.root}>
		<div>
			Link Version: <LinkVersion device={device} />
		</div>
		<div>
			<SvgLinkUsbDiagram />
		</div>
	</div>
);
