import { Check, ChevronDown } from 'lucide-react';

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { modelLabel, type ModelOption } from '@/lib/agent-session';

/** The model, as a quiet word in the composer's footer. It lists only the
 * models the Agent reported; with none, it just names what it runs. */
export function ModelPicker({
  models,
  value,
  reported,
  onChange,
}: {
  models: readonly ModelOption[];
  /** The picked model id; null = the Agent's default. */
  value: string | null;
  reported?: string | null;
  onChange: (id: string) => void;
}) {
  const label = modelLabel(models, value, reported);
  if (models.length === 0) {
    return label ? <span className="min-w-0 truncate" title={label}>{label}</span> : null;
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={`Model: ${label ?? 'default'}`}
          className="-mx-1 inline-flex min-w-0 items-center gap-0.5 rounded px-1 transition-colors hover:bg-bg-hover hover:text-text-secondary data-[state=open]:bg-bg-hover data-[state=open]:text-text-secondary"
        >
          <span className="min-w-0 truncate">{label ?? 'Model'}</span>
          <ChevronDown className="size-3 shrink-0" aria-hidden />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="top" className="max-h-80 w-64 overflow-y-auto">
        {models.map((model) => {
          const selected = value ? model.id === value : model.isDefault;
          return (
            <DropdownMenuItem key={model.id} onSelect={() => onChange(model.id)} className="items-start gap-2">
              <span className="min-w-0 flex-1">
                <span className="block truncate">{model.name}</span>
                {model.description && <span className="block truncate text-[11.5px] text-text-tertiary">{model.description}</span>}
              </span>
              <Check className={selected ? 'mt-0.5 size-3.5 shrink-0 text-accent' : 'mt-0.5 size-3.5 shrink-0 opacity-0'} aria-hidden />
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
