import * as C from "./WaitForConnection.css";

import OroLogo from "../components/OroLogo.jsx";
import Status from "../components/Status.jsx";

export default ({ enableConfigurator, device }) => (
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
				<button
					on:click={() => {
						enableConfigurator(true);
						device.open().catch((e) => {
							console.error("Failed to open device:", e);
							enableConfigurator(false);
							alert(
								"Failed to open device. Make sure you have granted permission and try again.",
							);
						});
					}}
				>
					Start Configurator
				</button>
			)}
		</div>
	</div>
);
