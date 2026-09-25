/** Tiny pub/sub so Titlebar can request a listing reload without importing GalleryPad. */

type GalleryRefreshListener = () => void;

const listeners = new Set<GalleryRefreshListener>();

export function subscribeGalleryRefresh(listener: GalleryRefreshListener): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

export function requestGalleryRefresh(): void {
	for (const listener of listeners) {
		listener();
	}
}
