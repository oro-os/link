import * as C from "./Connected.css";

import Link from "../components/Link";

export default ({ device }) => (
	<div class={C.root}>
		<Link device={device} />
	</div>
);
