import { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { Button } from '../components/ui/button';
import type { CloudSpace } from './client';
import { cloudSpaceRoute } from './routes';

export function CloudQuickSetup({
  space,
  canPublish,
}: {
  space: CloudSpace;
  canPublish: boolean;
}) {
  const [copied, setCopied] = useState(false);

  if (!canPublish) {
    return (
      <div className="h-full overflow-auto">
        <section className="w-full max-w-2xl" style={{ padding: '36px 56px 56px' }}>
          <p className="text-xs font-semibold uppercase tracking-[0.16em] text-accent">
            Shared Space
          </p>
          <h1 className="mt-4 font-serif text-[28px] font-semibold tracking-[-0.025em] text-text">
            Waiting for the first published version
          </h1>
          <p className="mt-3 max-w-lg text-sm leading-6 text-text-tertiary">
            The Space owner is preparing the knowledge that will appear here.
          </p>
        </section>
      </div>
    );
  }

  const origin = window.location.origin;
  const spaceUrl = `${origin}${cloudSpaceRoute(space.id)}`;
  const prompt = `Read ${origin}/skill.md. Ask me to choose a local CoWiki Space, then publish its first version to ${spaceUrl}`;

  const copyPrompt = async () => {
    await navigator.clipboard.writeText(prompt);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1800);
  };

  return (
    <div className="h-full overflow-auto">
      <section className="w-full max-w-3xl" style={{ padding: '36px 56px 56px' }}>
        <h1 className="page-title mb-0">
          Quick setup
        </h1>

        <ol className="mt-7 space-y-5">
          <SetupStep number="1" title="Install the CoWiki skill">
            Your Agent installs the latest version after asking permission.
          </SetupStep>
          <SetupStep number="2" title="Choose a local Space">
            You choose the folder that stays and remains editable on this device.
          </SetupStep>
          <SetupStep number="3" title="Publish with your Agent">
            The first version appears here. Future updates appear after review.
          </SetupStep>
        </ol>

        <div className="mt-8 overflow-hidden rounded-xl border border-border bg-panel">
          <p className="px-5 py-4 font-mono text-[12.5px] leading-5 text-text-secondary">
            {prompt}
          </p>
          <div className="flex justify-end border-t border-border bg-secondary/40 px-4 py-3">
            <Button
              type="button"
              size="sm"
              className="shrink-0 gap-2"
              onClick={() => void copyPrompt()}
            >
              {copied ? <Check size={14} /> : <Copy size={14} />}
              {copied ? 'Copied' : 'Copy prompt'}
            </Button>
          </div>
        </div>
      </section>
    </div>
  );
}

function SetupStep({
  number,
  title,
  children,
}: {
  number: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <li className="flex gap-3.5">
      <span className="grid size-6 shrink-0 place-items-center rounded-full bg-accent-soft text-xs font-semibold text-accent">
        {number}
      </span>
      <div className="pt-0.5">
        <p className="text-sm font-semibold text-text">{title}</p>
        <p className="mt-1 text-xs leading-5 text-text-tertiary">
          {children}
        </p>
      </div>
    </li>
  );
}
