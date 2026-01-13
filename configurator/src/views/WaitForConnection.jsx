import * as C from "./WaitForConnection.css";

import OroLogo from "../components/OroLogo.jsx";
import Status from "../components/Status.jsx";

export default ({ enableConfigurator }) => (
	<div class={C.root}>
		<div class={C.logo}>
			<OroLogo />
		</div>
		<div class={C.fadein}>
			{enableConfigurator() ? (
				<Status>
					Link configurator is waiting for a connection...
				</Status>
			) : (
				<button on:click={() => enableConfigurator(true)}>
					Start Configurator
				</button>
			)}
		</div>
	</div>
);
