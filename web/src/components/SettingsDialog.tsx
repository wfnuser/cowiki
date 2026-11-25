import { useState, useEffect, useCallback, type ReactNode } from 'react';
import { isSubmitEnter } from '@/lib/ime';
import { motion, AnimatePresence } from 'framer-motion';
import { AlertTriangle, Check, Clock, Cloud, Copy, Key, Loader2, Plus, Shield, Trash2 } from 'lucide-react';
import {
  Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { PluginSettings, SettingSwitch, settingsInputClass } from '@/plugins/PluginSettings';
import type { BuiltinPlugin } from '@/plugins/host';
import { MODULES, type ModuleId, type PersonalPreferences, type SpaceFeatures } from '@/lib/space-features';
import { cn } from '@/lib/utils';
import {
  listApiKeys, createApiKey, revokeApiKey,
  type ApiKeyInfo, type ApiKeyCreated,
} from '../api';
import type { AuthUser } from '../auth';
import {
  settingsTabs,
  type DefaultAgent,
} from '../lib/client-settings';
import { agentDefinition, SUPPORTED_AGENTS } from '../lib/agents';
import { loadTheme, setThemePreference, THEME_CHOICES, type ThemePreference } from '../lib/theme';

/** The open Space's settings, stored in the Space and shared with its members. */
export interface SpaceSettingsInput {
  name: string;
  features: SpaceFeatures;
  plugins: readonly BuiltinPlugin[];
  ready: boolean;
  configurationError?: string;
  /** Where the Space lives ("Local only", "Synced with Cloud") and its Cloud action. */
  place: { label: string; detail?: string; action: { label: string; onSelect: () => void } | null };
  onReload: () => Promise<void>;
  onSave: (features: SpaceFeatures) => Promise<void>;
}

export type SettingsGroup = 'space' | 'app';

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Which group to open on: the Space's settings or the app's. */
  group?: SettingsGroup;
  /** null when the open Space has no settings here (a Cloud Space in the browser). */
  space?: SpaceSettingsInput | null;
  /** Personal preferences: only this person, only this computer. */
  preferences: PersonalPreferences;
  onPreferencesChange: (preferences: PersonalPreferences) => void;
  clientMode: boolean;
  cloudConnected: boolean;
  auth: AuthUser | null;
  defaultAgent: DefaultAgent;
  onDefaultAgentChange: (agent: DefaultAgent) => void;
  onConnectCloud: () => void;
}

const itemFade = {
  initial: { opacity: 0, y: 12 },
  animate: { opacity: 1, y: 0, transition: { type: 'spring' as const, stiffness: 400, damping: 30 } },
  exit: { opacity: 0, y: -8, transition: { duration: 0.15 } },
};

// ── Sub-components ──

function KeyCardSkeleton() {
  return (
    <div className="flex items-center justify-between rounded-lg border px-4 py-3">
      <div className="space-y-2">
        <div className="h-4 w-24 animate-pulse rounded bg-muted" />
        <div className="h-3 w-40 animate-pulse rounded bg-muted" />
      </div>
      <div className="h-8 w-8 animate-pulse rounded bg-muted" />
    </div>
  );
}

function KeyCard({
  keyInfo,
  onRevoke,
  revoking,
}: {
  keyInfo: ApiKeyInfo;
  onRevoke: (id: string) => void;
  revoking: string | null;
}) {
  const isRevoking = revoking === keyInfo.id;

  return (
    <motion.div
      variants={itemFade}
      layout
      className="group flex items-center justify-between rounded-lg border bg-card px-4 py-3 transition-colors hover:border-border-hover"
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-foreground">{keyInfo.name}</p>
        <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
          <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-xs">
            {keyInfo.key_prefix}
          </code>
          <span>·</span>
          <span>{new Date(keyInfo.created_at).toLocaleDateString()}</span>
          {keyInfo.last_used_at && (
            <>
              <span>·</span>
              <span className="flex items-center gap-1">
                <Clock className="h-3 w-3" />
                {new Date(keyInfo.last_used_at).toLocaleDateString()}
              </span>
            </>
          )}
        </div>
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0 text-muted-foreground opacity-0 transition-[opacity,background-color,color] hover:bg-red-soft hover:text-red group-hover:opacity-100"
        disabled={isRevoking}
        onClick={() => onRevoke(keyInfo.id)}
      >
        {isRevoking ? (
          <Loader2 className="h-4 w-4 animate-spin" />
        ) : (
          <Trash2 className="h-4 w-4" />
        )}
      </Button>
    </motion.div>
  );
}

// ── Create Key Dialog ──

function CreateKeyDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
  const [step, setStep] = useState<'name' | 'reveal'>('name');
  const [name, setName] = useState('');
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revealedKey, setRevealedKey] = useState<ApiKeyCreated | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) {
      const t = setTimeout(() => {
        setStep('name');
        setName('');
        setError(null);
        setRevealedKey(null);
        setCopied(false);
      }, 200);
      return () => clearTimeout(t);
    }
  }, [open]);

  const handleCreate = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Name is required.');
      return;
    }
    setCreating(true);
    setError(null);
    try {
      const result = await createApiKey(trimmed);
      setRevealedKey(result);
      setStep('reveal');
      onCreated();
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to create key.';
      if (msg.includes('invalid API key') || msg.includes('Unauthorized')) {
        setError('Authentication failed. Your session may have expired — try logging in again.');
      } else {
        setError(msg);
      }
    } finally {
      setCreating(false);
    }
  };

  const handleCopy = async () => {
    if (!revealedKey) return;
    await navigator.clipboard.writeText(revealedKey.raw_key);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Create API Key</DialogTitle>
        </DialogHeader>

        {step === 'name' && (
          <div className="space-y-4">
            <div>
              <label htmlFor="key-name" className="text-sm font-medium">
                Key name
              </label>
              <input
                id="key-name"
                type="text"
                className="mt-1.5 w-full rounded-md border bg-background px-3 py-2 text-sm outline-none transition-colors placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                placeholder="e.g. CLI, CI/CD, Web"
                value={name}
                onChange={(e) => { setName(e.target.value); setError(null); }}
                onKeyDown={(e) => isSubmitEnter(e) && handleCreate()}
                autoFocus
              />
              {error && (
                <p className="mt-1.5 text-xs text-red">{error}</p>
              )}
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
              <Button size="sm" onClick={handleCreate} disabled={creating}>
                {creating ? (
                  <>
                    <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                    Creating...
                  </>
                ) : (
                  'Create'
                )}
              </Button>
            </div>
          </div>
        )}

        {step === 'reveal' && revealedKey && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            transition={{ type: 'spring', stiffness: 400, damping: 30 }}
            className="space-y-4"
          >
            <div className="relative overflow-hidden rounded-lg border border-border bg-bg-secondary p-4">
              <div className="relative space-y-3">
                <p className="text-xs font-medium text-muted-foreground">
                  Your API Key — copy it now!
                </p>
                <div className="flex items-center gap-2">
                  <code className="flex-1 select-all break-all rounded bg-background/80 px-3 py-2 font-mono text-sm backdrop-blur-sm">
                    {revealedKey.raw_key}
                  </code>
                  <motion.button
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.95 }}
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-background text-muted-foreground transition-colors hover:bg-accent hover:text-on-accent"
                    onClick={handleCopy}
                  >
                    <AnimatePresence mode="wait" initial={false}>
                      {copied ? (
                        <motion.div key="check" initial={{ scale: 0 }} animate={{ scale: 1 }} exit={{ scale: 0 }}>
                          <Check className="h-4 w-4" />
                        </motion.div>
                      ) : (
                        <motion.div key="copy" initial={{ scale: 0 }} animate={{ scale: 1 }} exit={{ scale: 0 }}>
                          <Copy className="h-4 w-4" />
                        </motion.div>
                      )}
                    </AnimatePresence>
                  </motion.button>
                </div>
                <div className="flex items-start gap-1.5 text-xs text-amber">
                  <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                  <span>This key will not be shown again. Store it somewhere safe.</span>
                </div>
              </div>
            </div>
            <div className="flex justify-end">
              <Button size="sm" onClick={() => onOpenChange(false)}>Done</Button>
            </div>
          </motion.div>
        )}
      </DialogContent>
    </Dialog>
  );
}

// ── Revoke Confirm Dialog ──

function RevokeConfirmDialog({
  open,
  keyName,
  revoking,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  keyName: string;
  revoking: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onCancel()}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Revoke API Key</DialogTitle>
        </DialogHeader>
        <div className="space-y-4">
          <p className="text-sm text-muted-foreground">
            Are you sure you want to revoke <span className="font-medium text-foreground">"{keyName}"</span>?
            This action cannot be undone.
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={onCancel} disabled={revoking}>Cancel</Button>
            <Button variant="destructive" size="sm" onClick={onConfirm} disabled={revoking}>
              {revoking ? (
                <><Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />Revoking...</>
              ) : (
                'Revoke'
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ── Main Settings Dialog ──

type Page = 'space-general' | `space-plugin:${string}` | 'app-general' | 'account' | 'keys';

/**
 * One Settings window, as in Notion or Linear: the open Space's settings
 * (stored in the Space, shared with everyone in it, saved together) and the
 * app's (only you, this computer, applied at once), side by side.
 */
export function SettingsDialog({
  open,
  onOpenChange,
  group = 'app',
  space,
  preferences,
  onPreferencesChange,
  clientMode,
  cloudConnected,
  auth,
  defaultAgent,
  onDefaultAgentChange,
  onConnectCloud,
}: SettingsDialogProps) {
  const availableTabs = settingsTabs({ clientMode, cloudConnected });
  const pluginPages = (space?.plugins ?? []).filter((plugin) => plugin.settings?.fields.length);
  const firstPage: Page = group === 'space' && space ? 'space-general' : availableTabs.includes('client') ? 'app-general' : 'account';
  const [page, setPage] = useState<Page>(firstPage);
  const [draft, setDraft] = useState<SpaceFeatures | null>(space?.features ?? null);
  const [saving, setSaving] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [spaceError, setSpaceError] = useState('');
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [theme, setTheme] = useState<ThemePreference>(loadTheme);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyInfo | null>(null);
  const [revoking, setRevoking] = useState<string | null>(null);

  // Opening starts from the saved settings, on the group that was asked for.
  const spaceFeatures = space?.features;
  useEffect(() => {
    if (!open) return;
    const pending = window.setTimeout(() => {
      setPage(firstPage);
      setDraft(spaceFeatures ?? null);
      setTheme(loadTheme());
      setSpaceError('');
    }, 0);
    return () => window.clearTimeout(pending);
    // Only when the window opens or the saved settings change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, spaceFeatures]);

  const fetchKeys = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listApiKeys();
      setKeys(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to load API keys.';
      if (msg.includes('invalid API key') || msg.includes('Unauthorized')) {
        setError('Session expired. Please log in again to manage API keys.');
      } else {
        setError(msg);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open || !cloudConnected) return;
    const pending = window.setTimeout(() => { void fetchKeys(); }, 0);
    return () => window.clearTimeout(pending);
  }, [open, cloudConnected, fetchKeys]);

  const handleRevoke = async () => {
    if (!revokeTarget) return;
    setRevoking(revokeTarget.id);
    try {
      await revokeApiKey(revokeTarget.id);
      setKeys((prev) => prev.filter((k) => k.id !== revokeTarget.id));
      setRevokeTarget(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'Failed to revoke key.';
      if (msg.includes('invalid API key') || msg.includes('Unauthorized')) {
        setError('Session expired. Please log in again.');
      } else {
        setError(msg);
      }
    } finally {
      setRevoking(null);
    }
  };

  const dirty = !!space && !!draft && JSON.stringify(draft) !== JSON.stringify(space.features);
  const saveSpace = async () => {
    if (!space || !draft) return;
    setSaving(true);
    setSpaceError('');
    try { await space.onSave(draft); }
    catch (caught) { setSpaceError(caught instanceof Error ? caught.message : String(caught)); }
    finally { setSaving(false); }
  };
  const reloadSpace = async () => {
    if (!space) return;
    setReloading(true);
    setSpaceError('');
    try { await space.onReload(); }
    catch (caught) { setSpaceError(caught instanceof Error ? caught.message : String(caught)); }
    finally { setReloading(false); }
  };
  const setModule = (id: ModuleId, enabled: boolean) => {
    setDraft((previous) => previous && ({ ...previous, modules: { ...previous.modules, [id]: enabled } }));
  };

  const navItem = (id: Page, label: string) => (
    <button
      key={id}
      type="button"
      aria-current={page === id ? 'page' : undefined}
      onClick={() => setPage(id)}
      className={cn(
        'flex h-7 w-full items-center rounded-md px-2.5 text-left text-small transition-colors',
        page === id ? 'bg-bg-active/70 font-medium text-text' : 'text-text-secondary hover:bg-bg-hover',
      )}
    >
      {label}
    </button>
  );
  const pluginPage = page.startsWith('space-plugin:') ? pluginPages.find((plugin) => `space-plugin:${plugin.id}` === page) : undefined;
  const title = page === 'space-general' ? 'General'
    : pluginPage ? pluginPage.settings!.title
    : page === 'app-general' ? 'General'
    : page === 'account' ? 'Account' : 'API Keys';
  const onSpacePage = page.startsWith('space-');

  let body: ReactNode = null;
  if (onSpacePage && space && draft) {
    body = (
      <>
        {!space.ready && <p role="status" className="pb-2 pt-1 text-small text-text-tertiary">{space.configurationError || 'Loading Space settings…'}</p>}
        <fieldset disabled={!space.ready || saving || reloading} style={{ border: 0, padding: 0, margin: 0, display: 'contents' }}>
          {page === 'space-general' ? (
            <>
              <section className="settings-section is-first">
                <h3 className="settings-heading">Space</h3>
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-small text-text">{space.name}</p>
                    <p className="truncate text-meta text-text-tertiary">{space.place.label}{space.place.detail ? ` · ${space.place.detail}` : ''}</p>
                  </div>
                  {space.place.action && (
                    <Button size="sm" variant="outline" onClick={() => { onOpenChange(false); space.place.action!.onSelect(); }}>
                      {space.place.action.label}
                    </Button>
                  )}
                </div>
              </section>
              <section className="settings-section">
                <h3 className="settings-heading mb-0">Modules</h3>
                <p className="mb-1 text-meta leading-[1.5] text-text-tertiary">Turning a module off only hides it; your notes stay.</p>
                <div>
                  <PluginSettings plugins={space.plugins} features={draft} preferences={preferences} onFeatures={setDraft} onPreferences={onPreferencesChange} part="toggles" />
                  {MODULES.filter((module) => !space.plugins.some((plugin) => plugin.id === module.id)).map((module) => (
                    <SettingSwitch
                      key={module.id}
                      label={module.label}
                      description={module.description}
                      checked={draft.modules[module.id]}
                      onChange={(enabled) => setModule(module.id, enabled)}
                    />
                  ))}
                </div>
              </section>
              <section className="settings-section">
                <h3 className="settings-heading">Templates</h3>
                <div className="grid grid-cols-[112px_minmax(0,1fr)] items-center gap-x-3">
                  <label htmlFor="templates-folder" className="text-small text-text-secondary">Folder</label>
                  <input
                    id="templates-folder"
                    className={settingsInputClass}
                    value={draft.templates.folder}
                    placeholder="Templates"
                    onChange={(event) => setDraft((previous) => previous && ({ ...previous, templates: { folder: event.target.value } }))}
                  />
                </div>
                <p className="mt-2.5 text-meta leading-[1.55] text-text-tertiary">
                  Pages in this folder are offered when you create a page. They support <code className="font-mono">{'{{title}}'}</code>, <code className="font-mono">{'{{date}}'}</code> and <code className="font-mono">{'{{time}}'}</code>.
                </p>
              </section>
            </>
          ) : pluginPage ? (
            <PluginSettings plugins={[pluginPage]} features={draft} preferences={preferences} onFeatures={setDraft} onPreferences={onPreferencesChange} part="fields" />
          ) : null}
        </fieldset>
      </>
    );
  } else if (page === 'app-general') {
    body = (
      <>
        <section className="settings-section is-first">
          <h3 className="settings-heading">Appearance</h3>
          <div className="grid grid-cols-3 gap-0.5 rounded-lg bg-bg-secondary p-0.5 ring-1 ring-inset ring-border-soft" role="radiogroup" aria-label="Appearance">
            {THEME_CHOICES.map(([value, label]) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={theme === value}
                onClick={() => { setTheme(value); setThemePreference(value); }}
                className={`h-7 rounded-md px-2 text-small transition-colors ${
                  theme === value
                    ? 'bg-panel font-medium text-text shadow-[var(--shadow-chip)] ring-1 ring-border'
                    : 'text-text-tertiary hover:text-text-secondary'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </section>
        <section className="settings-section">
          <h3 className="settings-heading">Default Agent</h3>
          <p className="-mt-1.5 mb-2.5 text-small text-text-tertiary">New sessions start with it; the last one you use becomes the default.</p>
          <div className="grid grid-cols-2 gap-1.5">
            {SUPPORTED_AGENTS.map((agent) => {
              const selected = defaultAgent === agent;
              const definition = agentDefinition(agent);
              return (
                <button
                  key={agent}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => onDefaultAgentChange(agent)}
                  className={`flex items-center justify-between rounded-lg border px-3 py-2 text-left transition-colors ${
                    selected
                      ? 'border-text-tertiary bg-bg-active/60 text-text'
                      : 'border-border-soft text-text-secondary hover:bg-bg-hover'
                  }`}
                >
                  <span className="text-small font-medium">{definition.displayName}</span>
                  {selected && <Check className="size-3.5 text-text" aria-hidden />}
                </button>
              );
            })}
          </div>
        </section>
        <section className="settings-section">
          <h3 className="settings-heading mb-0">History</h3>
          <div>
            {space && <PluginSettings plugins={space.plugins} features={space.features} preferences={preferences} onFeatures={() => {}} onPreferences={onPreferencesChange} part="preferences" />}
            <SettingSwitch
              label="Save checkpoints automatically"
              description="After 5 quiet minutes and when CoWiki quits. Kept in History on this computer."
              checked={preferences.autoCheckpoint}
              onChange={(autoCheckpoint) => onPreferencesChange({ ...preferences, autoCheckpoint })}
            />
          </div>
        </section>
      </>
    );
  } else if (page === 'account') {
    body = cloudConnected && auth ? (
      <div className="space-y-3 pt-1">
        <div>
          <label className="text-xs font-medium text-muted-foreground">Name</label>
          <p className="text-sm">{auth.name}</p>
        </div>
        <div>
          <label className="text-xs font-medium text-muted-foreground">User ID</label>
          <p className="font-mono text-xs text-muted-foreground">{auth.id}</p>
        </div>
      </div>
    ) : (
      <div className="flex min-h-[280px] flex-col items-center justify-center text-center">
        <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-accent-soft">
          <Cloud className="h-5 w-5 text-accent" />
        </div>
        <p className="text-sm font-medium">Connect to CoWiki Cloud</p>
        <p className="mt-1 max-w-xs text-xs leading-relaxed text-muted-foreground">
          Keep local Spaces account-free. Sign in only when you want to publish or join a shared Space.
        </p>
        <Button
          className="mt-4 bg-accent text-on-accent hover:bg-accent-hover"
          onClick={() => {
            onOpenChange(false);
            onConnectCloud();
          }}
        >
          Sign up / Sign in
        </Button>
      </div>
    );
  } else if (page === 'keys') {
    body = (
      <div className="flex flex-col">
        <div className="mb-4 flex items-start gap-2.5 rounded-lg border border-amber/30 bg-amber-soft px-3.5 py-2.5 text-sm text-amber-ink">
          <Shield className="mt-0.5 h-4 w-4 shrink-0" />
          <span>API keys are only shown once upon creation. Copy and store them securely.</span>
        </div>
        <div className="mb-4 flex items-center justify-between">
          <h4 className="text-sm font-medium">Your API Keys</h4>
          <Button size="sm" className="gap-1.5" onClick={() => setShowCreate(true)}>
            <Plus className="h-3.5 w-3.5" />
            New Key
          </Button>
        </div>
        {loading && (
          <div className="space-y-2">
            <KeyCardSkeleton />
            <KeyCardSkeleton />
          </div>
        )}
        {!loading && error && (
          <div className="flex flex-col items-center gap-2 rounded-lg border border-red/20 bg-red-soft py-6 text-center">
            <p className="text-sm text-red">{error}</p>
            <Button variant="outline" size="sm" onClick={fetchKeys}>Retry</Button>
          </div>
        )}
        {!loading && !error && keys.length === 0 && (
          <div className="flex flex-1 flex-col items-center justify-center py-10 text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-muted">
              <Key className="h-5 w-5 text-muted-foreground" />
            </div>
            <p className="text-sm font-medium text-foreground">No API keys yet</p>
            <p className="mt-1 text-xs text-muted-foreground">Create your first API key to get started.</p>
          </div>
        )}
        {!loading && !error && keys.length > 0 && (
          <motion.div initial="initial" animate="animate" className="space-y-2">
            <AnimatePresence mode="popLayout">
              {keys.map((k) => (
                <KeyCard
                  key={k.id}
                  keyInfo={k}
                  onRevoke={(id) => {
                    const target = keys.find((kk) => kk.id === id);
                    if (target) setRevokeTarget(target);
                  }}
                  revoking={revoking}
                />
              ))}
            </AnimatePresence>
          </motion.div>
        )}
      </div>
    );
  }

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="top-1/2 flex h-[min(620px,calc(100vh-3rem))] -translate-y-1/2 flex-row gap-0 overflow-hidden p-0 sm:max-w-[800px]">
          <nav aria-label="Settings" className="flex w-[196px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-border-soft bg-bg-secondary px-2 py-4">
            <DialogTitle className="px-2.5 pb-3 text-small font-semibold text-text">Settings</DialogTitle>
            {space && (
              <>
                <p className="truncate px-2.5 pb-1 text-meta font-medium text-text-tertiary" title={space.name}>{space.name}</p>
                {navItem('space-general', 'General')}
                {pluginPages.map((plugin) => navItem(`space-plugin:${plugin.id}`, plugin.settings!.title))}
                <div className="h-4" />
              </>
            )}
            <p className="px-2.5 pb-1 text-meta font-medium text-text-tertiary">App</p>
            {availableTabs.includes('client') && navItem('app-general', 'General')}
            {navItem('account', 'Account')}
            {availableTabs.includes('keys') && navItem('keys', 'API Keys')}
          </nav>

          <div className="flex min-w-0 flex-1 flex-col">
            <DialogHeader className="shrink-0 gap-0.5 px-7 pb-2 pt-5 pr-12">
              <h2 className="text-lead font-semibold leading-6 text-text">{title}</h2>
              <DialogDescription className="text-meta leading-[1.5] text-text-tertiary">
                {onSpacePage && space
                  ? <>Applies to everyone in <span className="font-medium text-text-secondary">{space.name}</span>.</>
                  : 'Only for you, on this computer.'}
              </DialogDescription>
            </DialogHeader>
            <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-4">{body}</div>
            {space && (dirty || spaceError || (onSpacePage && space.configurationError)) && (
              <div className="flex shrink-0 items-center gap-2 border-t border-border-soft bg-panel px-7 py-3">
                {spaceError
                  ? <p role="alert" className="min-w-0 flex-1 truncate text-small text-red" title={spaceError}>{spaceError}</p>
                  : <span className="min-w-0 flex-1 truncate text-small text-text-tertiary">{dirty ? (onSpacePage ? 'Unsaved changes' : `Unsaved changes in ${space.name}`) : ''}</span>}
                {(space.configurationError || spaceError) && <Button size="sm" variant="outline" onClick={() => { void reloadSpace(); }} disabled={saving || reloading}>{reloading ? 'Reloading…' : 'Reload settings'}</Button>}
                {dirty && <Button size="sm" variant="outline" onClick={() => setDraft(space.features)} disabled={saving || reloading}>Discard</Button>}
                {dirty && <Button size="sm" onClick={() => { void saveSpace(); }} disabled={saving || reloading || !space.ready}>{saving ? 'Saving…' : 'Save'}</Button>}
              </div>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {cloudConnected && (
        <>
          <CreateKeyDialog
            open={showCreate}
            onOpenChange={setShowCreate}
            onCreated={fetchKeys}
          />

          <RevokeConfirmDialog
            open={!!revokeTarget}
            keyName={revokeTarget?.name || ''}
            revoking={!!revoking}
            onConfirm={handleRevoke}
            onCancel={() => setRevokeTarget(null)}
          />
        </>
      )}
    </>
  );
}
