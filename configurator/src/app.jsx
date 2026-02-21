import S from "@surplus/s";

import "./global.css";

import Root from "./views/Root.jsx";

import { Device } from "./lib/device";

// Mount the root component to the document body
S.root(() => {
	const state = (window.State = {
		fatalError: S.value(undefined),
		enableConfigurator: S.value(false),
	});

	state.device = new Device();

	window.addEventListener("error", (e) =>
		state.fatalError(e.error ?? e ?? "(unknown error)"),
	);
	window.addEventListener("unhandledrejection", (e) =>
		state.fatalError(e.reason ?? e ?? "(unknown error)"),
	);

	document.body.prepend(<Root {...state} />);
});
