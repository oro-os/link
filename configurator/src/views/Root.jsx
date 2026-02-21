import S from "@surplus/s";
import css from "@surplus/css";

import * as C from "./Root.css";

import FatalError from "./FatalError.jsx";
import WaitForConnection from "./WaitForConnection.jsx";
import Connected from "./Connected.jsx";

export default ({ device, fatalError, ...state }) => {
	const currentView = S.value(WaitForConnection);
	S(() =>
		currentView(
			(() => {
				if (fatalError())
					return () => <FatalError error={fatalError()} />;
				if (device.online()) return Connected;
				return WaitForConnection;
			})(),
		),
	);

	let switching = false;
	const switchingClass = S.value(null);
	const visibleView = S.value(S.sample(currentView));
	S.on(currentView, () => {
		if (switching) return;
		switching = true;
		switchingClass(C.hideContent);

		setTimeout(() => {
			switching = false;
			// NOTE: Don't freeze here; views might take a moment to render.
			visibleView(currentView());
			switchingClass(null);
		}, 200);
	});

	return (
		<div class={C.root}>
			<div fn={css(C.content, switchingClass)}>
				{visibleView()({ device, ...state })}
			</div>
		</div>
	);
};
