// Running one backend command, and saying whether it worked.
//
// The boolean is the point. A failure is reported by putting the message on screen
// rather than by rejecting, so a caller that treats "the promise resolved" as "it
// saved" tells the user their edit landed while the banner beside it says it did not.

export interface Guarded {
	onBusy: (busy: boolean) => void;
	onError: (message: string) => void;
}

export const message = (error: unknown): string =>
	error instanceof Error ? error.message : String(error);

export const guarded = async (work: () => Promise<void>, report: Guarded): Promise<boolean> => {
	report.onBusy(true);
	try {
		await work();
		return true;
	} catch (error) {
		report.onError(message(error));
		return false;
	} finally {
		report.onBusy(false);
	}
};
