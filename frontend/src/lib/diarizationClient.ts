/**
 * Production singleton for the diarization seam.
 *
 * Kept in its own module (mirroring `audiocppClient.ts`) so the pure logic in
 * `diarization.ts` stays importable by plain unit tests.
 */

import { TauriDiarizationClient, type DiarizationClient } from './diarization';

export const diarizationClient: DiarizationClient = new TauriDiarizationClient();
