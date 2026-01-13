import * as C from "./OroLogo.css";

export default () => (
	<svg
		width="100%"
		height="100%"
		viewBox="-5 -5 144 144"
		version="1.1"
		class={C.root}
		xmlns="http://www.w3.org/2000/svg"
	>
		<defs>
			<clipPath id="planet-clip">
				<path
					d="M-200,-200L500,-200L500,500L-200,500ZM22.286,66.506a44.5,44.5,0,1,1,89,0,44.5,44.5,0,1,1,-89,0"
					clip-rule="evenodd"
				/>
			</clipPath>
			<clipPath id="moon-clip">
				<path
					d="M-800,-800L800,-800L800,800L-800,800ZM-14,0a14,14,0,1,1,28,0,14,14,0,1,1,-28,0"
					clip-rule="evenodd"
					class={C.moonOffset}
				/>
			</clipPath>
			<clipPath id="moon-clip-planet">
				<path
					d="M-800,-800L800,-800L800,800L-800,800ZM-14,0a14,14,0,1,1,28,0,14,14,0,1,1,-28,0"
					clip-rule="evenodd"
					class={C.moonOffsetFront}
				/>
			</clipPath>
		</defs>

		<path
			d="M81.801,24.364C97.552,14.67 110.872,11.355 116.526,17.009C122.187,22.67 118.855,36.019 109.132,51.797"
			style="fill:none;stroke:currentColor;stroke-width:1.89px"
			clip-path="url(#moon-clip)"
		/>
		<path
			d="M52.192,108.688C36.469,118.351 23.177,121.65 17.531,116.004C11.834,110.307 15.243,96.826 25.109,80.918"
			style="fill:none;stroke:currentColor;stroke-width:1.89px"
			clip-path="url(#moon-clip)"
		/>
		<g clip-path="url(#planet-clip)">
			<circle
				class={C.moonOffset}
				cx="0"
				cy="0"
				r="14"
				style="fill:none;stroke:currentColor;stroke-width:7.12px"
			/>
		</g>
		<circle
			cx="66.786"
			cy="66.506"
			r="44.5"
			style="stroke:currentColor;stroke-width:7.12px;fill: none"
			clip-path="url(#moon-clip-planet)"
		/>
		<path
			d="M109.132,51.797C102.999,61.749 94.322,72.668 83.756,83.234C73.145,93.845 62.178,102.55 52.192,108.688"
			style="fill:none;stroke:currentColor;stroke-width:1.89px"
			clip-path="url(#moon-clip)"
		/>
		<circle
			class={C.moonOffsetFront}
			cx="0"
			cy="0"
			r="14"
			style="fill:none;stroke:currentColor;stroke-width:7.12px"
		/>
	</svg>
);
