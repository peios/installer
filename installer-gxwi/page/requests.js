// This is the wire-message limit, including UTF-8 bytes rather than JS characters.
export const MAX_REQUEST_BYTES = 1024 * 1024;

export function encodeRequest(request, sequence) {
    const text = JSON.stringify({ ...request, seq: sequence });
    return new TextEncoder().encode(text).length <= MAX_REQUEST_BYTES ? text : null;
}

export function refusalMessage(code) {
    if (code === 1009) {
        return "The request was too large or could not be accepted. Shorten the input and try again.";
    }
    if (code === 1012) {
        return "The conversation changed. Pending input was not replayed; check the current page before trying again.";
    }
    return "";
}

// A retained display snapshot is not authority to send on a new connection.
export function canAsk(request, view, freshSnapshot) {
    return !!(freshSnapshot && view && (view.seq || request.again));
}
