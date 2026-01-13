import S from "@surplus/s";

import "./global.css";

import { device } from "./lib/usb-controller.mjs";

import Root from "./views/Root.jsx";

// Mount the root component to the document body
S.root(() => {
	const state = (window.State = {
		fatalError: S.value(undefined),
		enableConfigurator: S.value(false),
	});

	state.usbDevice = device({ enable: state.enableConfigurator });

	window.addEventListener("error", (e) =>
		state.fatalError(e.error ?? e ?? "(unknown error)"),
	);
	window.addEventListener("unhandledrejection", (e) =>
		state.fatalError(e.reason ?? e ?? "(unknown error)"),
	);

	document.body.prepend(<Root {...state} />);
});
