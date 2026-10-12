/** Move a node under document.body so fixed overlays escape titlebar backdrop-filter. */
export function portal(node: HTMLElement) {
	document.body.appendChild(node);
	return {
		destroy() {
			node.remove();
		}
	};
}
