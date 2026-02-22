import S from "@surplus/s";

import * as C from "./LinkLight.css";

let counter = 0;

export default ({
	r,
	g,
	b,
	x,
	y,
	onStartDebug,
	onEndDebug,
	mouseRadius = 10,
}) => {
	const id = `link-light-${counter++}`;
	const color = S(() => `rgb(${S.$(r)}, ${S.$(g)}, ${S.$(b)})`);
	const lum = S(() => Math.max(S.$(r), Math.max(S.$(g), S.$(b))) / 255.0);
	const over = S.value(false);
	return (
		<div class={C.root} style={`left: ${x}; top: ${y}`}>
			<svg
				viewBox="0 0 100 100"
				width="50"
				height="50"
				xmlns="http://www.w3.org/2000/svg"
			>
				<defs>
					<radialGradient id={id}>
						<stop
							offset="0%"
							stop-color={color()}
							stop-opacity={0.95 * lum()}
						/>
						<stop
							offset="4%"
							stop-color={color()}
							stop-opacity={0.95 * lum()}
						/>
						<stop
							offset="25%"
							stop-color={color()}
							stop-opacity={0.25 * lum()}
						/>
						<stop
							offset="100%"
							stop-color={color()}
							stop-opacity="0"
						/>
					</radialGradient>
				</defs>
				<circle
					cx="50"
					cy="50"
					r={mouseRadius}
					fill={over() ? "#68D4D067" : "#68D4D030"}
					class={C.handle}
					on:mouseenter={() => {
						over(true);
						onStartDebug();
					}}
					on:mouseleave={() => {
						over(false);
						onEndDebug();
					}}
				/>
				<circle cx="50" cy="50" r="42" fill={`url(#${id})`} />
			</svg>
		</div>
	);
};
