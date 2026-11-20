import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

interface TranscribeCppRawModel {
  name: string;
  display_name: string;
  size_mb: number;
  status: unknown;
}

let transcribeCppModels: TranscribeCppRawModel[] = [];
let transcribeCppFails = false;
const invokeCalls: string[] = [];

const invoke = mock(async (command: string): Promise<unknown> => {
  invokeCalls.push(command);
  if (command === 'transcribecpp_get_available_models') {
    if (transcribeCppFails) throw new Error('transcribe.cpp model manager not initialized');
    return transcribeCppModels;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));

const { useTranscriptionModels } = await import('../../src/hooks/useTranscriptionModels');

function Harness({ config }: { config: any }) {
  const hook = useTranscriptionModels(config);
  (Harness as any).latest = hook;
  return null;
}

let renderer: ReactTestRenderer | undefined;

async function renderHook(config: any) {
  await act(async () => {
    renderer = create(<Harness config={config} />);
  });
}

/// The hook object is recreated every render; always read the latest one
/// after an `act` block.
function latest() {
  return (Harness as any).latest as ReturnType<typeof useTranscriptionModels>;
}

beforeEach(() => {
  transcribeCppModels = [];
  transcribeCppFails = false;
  invokeCalls.length = 0;
  renderer?.unmount();
  renderer = undefined;
});

describe('useTranscriptionModels', () => {
  test('lists installed transcribeCpp models and nothing else', async () => {
    transcribeCppModels = [
      { name: 'parakeet-ultra-Q8_0', display_name: 'Parakeet Ultra', size_mb: 706, status: 'Available' },
      // A partial download must never be handed to a batch job.
      { name: 'gigaam-v3-ctc-Q8_0', display_name: 'GigaAM v3 CTC', size_mb: 259, status: 'Downloading' },
    ];

    await renderHook({ provider: 'transcribeCpp', model: 'parakeet-ultra-Q8_0' });
    await act(async () => {
      await latest().fetchModels();
    });

    const providers = latest().availableModels.map((m) => `${m.provider}:${m.name}`);
    expect(providers).toEqual(['transcribeCpp:parakeet-ultra-Q8_0']);
    // The dialogs read the one catalog; removed engines are never queried.
    expect(invokeCalls).toEqual(['transcribecpp_get_available_models']);
  });

  test('selects the configured transcribeCpp model by default', async () => {
    transcribeCppModels = [
      { name: 'parakeet-ultra-Q8_0', display_name: 'Parakeet Ultra', size_mb: 706, status: 'Available' },
      { name: 'gigaam-v3-ctc-Q8_0', display_name: 'GigaAM v3 CTC', size_mb: 259, status: 'Available' },
    ];

    await renderHook({ provider: 'transcribeCpp', model: 'gigaam-v3-ctc-Q8_0' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().selectedModelKey).toBe('transcribeCpp:gigaam-v3-ctc-Q8_0');
  });

  test('a removed engine in the saved config still matches its model by name', async () => {
    // A config that predates the soft migration names a removed provider; the
    // only catalog is transcribe.cpp, so the model match is by name alone.
    transcribeCppModels = [
      { name: 'parakeet-ultra-Q8_0', display_name: 'Parakeet Ultra', size_mb: 706, status: 'Available' },
    ];

    await renderHook({ provider: 'parakeet', model: 'parakeet-ultra-Q8_0' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().selectedModelKey).toBe('transcribeCpp:parakeet-ultra-Q8_0');
  });

  test('falls back to the first installed model when the configured one is missing', async () => {
    transcribeCppModels = [
      { name: 'parakeet-ultra-Q8_0', display_name: 'Parakeet Ultra', size_mb: 706, status: 'Available' },
    ];

    await renderHook({ provider: 'transcribeCpp', model: 'not-installed' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().selectedModelKey).toBe('transcribeCpp:parakeet-ultra-Q8_0');
  });

  test('a manager failure leaves no models and no selection', async () => {
    transcribeCppFails = true;

    await renderHook({ provider: 'transcribeCpp', model: 'parakeet-ultra-Q8_0' });
    await act(async () => {
      await latest().fetchModels();
    });

    expect(latest().availableModels).toHaveLength(0);
    expect(latest().selectedModelKey).toBe('');
  });
});
