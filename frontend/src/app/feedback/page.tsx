'use client';

import { ArrowLeft, ArrowUpRight } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

// TODO(brand): Discord/social/PRO cards removed during the Samobranka -> Samobranka rebrand.
// Re-add community links here once samobranka.ai channels exist.
const cards = [
  {
    title: 'Report a bug or suggest a feature',
    description: 'Report a bug or suggest a feature on GitHub. Your feedback helps shape Samobranka.',
    links: [{ label: 'Open GitHub issues', url: 'https://github.com/timurmazitov/samobranka/issues/new/choose' }],
  },
];

async function openExternalLink(url: string) {
  try {
    await invoke('open_external_url', { url });
  } catch (error) {
    console.error('Failed to open feedback link:', error);
    toast.error('Could not open this link. Please check your default browser and try again.');
  }
}

export default function FeedbackPage() {
  const router = useRouter();

  return (
    <div className="h-screen overflow-y-auto bg-gray-50">
      <div className="mx-auto w-full max-w-6xl px-8 py-8">
        <button
          type="button"
          onClick={() => router.back()}
          className="flex items-center gap-2 rounded text-gray-600 transition-colors hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2"
        >
          <ArrowLeft aria-hidden="true" className="h-4 w-4" />
          Back
        </button>
        <h1 className="mt-6 mb-8 text-2xl font-medium text-gray-900">Send Feedback</h1>

        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,300px),1fr))] gap-5">
          {cards.map(card => (
            <article key={card.title} className="flex min-h-40 flex-col rounded-lg border border-gray-200 bg-white p-5 pb-3">
              <h2 className="text-lg font-semibold leading-6 text-gray-900">{card.title}</h2>
              <p className="mt-2 text-sm leading-5 text-gray-500">{card.description}</p>
              <div className="mt-auto flex flex-wrap items-center gap-x-6">
                {card.links.map(link => (
                  <button
                    key={link.url}
                    type="button"
                    onClick={() => void openExternalLink(link.url)}
                    aria-label={`${link.label}`}
                    title={`${link.label}`}
                    className={`inline-flex min-h-11 items-center justify-center gap-1 rounded text-sm font-semibold text-gray-700 transition-colors hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2 ${'icon' in link ? 'min-w-11' : ''}`}
                  >
                    {'icon' in link ? (
                      <svg aria-hidden="true" focusable="false" className="h-5 w-5">
                        <use href={`/social-icons.svg#${link.icon}`} />
                      </svg>
                    ) : (
                      <>
                        {link.label}
                        <ArrowUpRight aria-hidden="true" className="h-3 w-3" />
                      </>
                    )}
                  </button>
                ))}
              </div>
            </article>
          ))}
        </div>
      </div>
    </div>
  );
}
