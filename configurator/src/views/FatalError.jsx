export default ({ error }) => (
	<div>
		<div>
			<h1>Fatal Error</h1>
		</div>
		<div>A fatal error occurred. Refresh to try again.</div>
		<div>
			<blockquote>
				{error?.stack || error?.toString?.() || "(unknown error)"}
			</blockquote>
		</div>
	</div>
);
