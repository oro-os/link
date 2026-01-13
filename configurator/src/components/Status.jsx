import * as C from "./Status.css";

export default ({ children, class: className, ...rest }) => (
	<span class={className} {...rest}>
		<span class={C.fade}>{children}</span>
	</span>
);
