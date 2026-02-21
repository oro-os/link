import esbuild from "esbuild";
import { surplus, surplusCss } from "@surplus/esbuild";
import fsp from "node:fs/promises";

const isProduction = process.env.NODE_ENV === "production";

// Build JSX application
await esbuild.build({
	entryPoints: ["src/app.jsx"],
	bundle: true,
	minify: isProduction,
	format: "esm",
	sourcemap: !isProduction,
	outfile: "pkg/app.js",
	plugins: [surplus(), surplusCss()],
});

// Copy static files
await fsp.copyFile("src/index.html", "pkg/index.html");
await fsp.copyFile("wasm/linkproto_bg.wasm", "pkg/linkproto_bg.wasm");
