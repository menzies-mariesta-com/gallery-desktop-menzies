type Listener = (available: boolean) => void;

let available = false;
const listeners = new Set<Listener>();

export function recognitionAvailable(): boolean {
	return available;
}

export function publishRecognitionAvailable(next: boolean): void {
	available = next;
	for (const listener of listeners) listener(next);
}

export function subscribeRecognitionAvailable(listener: Listener): () => void {
	listeners.add(listener);
	listener(available);
	return () => {
		listeners.delete(listener);
	};
}
