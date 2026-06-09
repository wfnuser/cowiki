import { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Key, Copy, Check, Trash2, Plus, Loader2, AlertTriangle, Shield, Clock } from 'lucide-react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import {
  listApiKeys, createApiKey, revokeApiKey,
  type ApiKeyInfo, type ApiKeyCreated,
} from '../api';
import { getStoredAuth } from '../auth';

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
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
      className="group flex items-center justify-between rounded-lg border bg-card px-4 py-3 transition-colors hover:border-border-hover hover:bg-accent/30"
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
        className="h-8 w-8 shrink-0 text-muted-foreground opacity-0 transition-all hover:bg-red-50 hover:text-red-600 group-hover:opacity-100"
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
                onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
                autoFocus
              />
              {error && (
                <p className="mt-1.5 text-xs text-red-600">{error}</p>
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
            <div className="relative overflow-hidden rounded-lg border border-border bg-gradient-to-br from-accent/5 to-accent/10 p-4">
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
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-background text-muted-foreground transition-colors hover:bg-accent hover:text-white"
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
                <div className="flex items-start gap-1.5 text-xs text-amber-700">
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

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [auth] = useState(() => getStoredAuth());
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyInfo | null>(null);
  const [revoking, setRevoking] = useState<string | null>(null);

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
    if (open) fetchKeys();
  }, [open, fetchKeys]);

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

  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Settings</DialogTitle>
          </DialogHeader>

          <Tabs defaultValue="profile" className="w-full">
            <TabsList className="w-full">
              <TabsTrigger value="profile" className="flex-1">Profile</TabsTrigger>
              <TabsTrigger value="keys" className="flex-1">API Keys</TabsTrigger>
            </TabsList>

            <TabsContent value="profile" className="mt-4 space-y-4 data-[state=inactive]:hidden" forceMount style={{ minHeight: 320 }}>
              <div className="space-y-3">
                <div>
                  <label className="text-xs font-medium text-muted-foreground">Name</label>
                  <p className="text-sm">{auth?.name || '—'}</p>
                </div>
                <div>
                  <label className="text-xs font-medium text-muted-foreground">User ID</label>
                  <p className="font-mono text-xs text-muted-foreground">{auth?.id || '—'}</p>
                </div>
              </div>
            </TabsContent>

            <TabsContent value="keys" className="mt-4 flex flex-col data-[state=inactive]:hidden" forceMount style={{ minHeight: 320 }}>
              <div className="flex items-start gap-2.5 rounded-lg border border-amber-200 bg-amber-50 px-3.5 py-2.5 text-sm text-amber-800 mb-4">
                <Shield className="mt-0.5 h-4 w-4 shrink-0" />
                <span>API keys are only shown once upon creation. Copy and store them securely.</span>
              </div>

              <div className="flex items-center justify-between mb-4">
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
                <div className="flex flex-col items-center gap-2 rounded-lg border border-red-200 bg-red-50 py-6 text-center">
                  <p className="text-sm text-red-700">{error}</p>
                  <Button variant="outline" size="sm" onClick={fetchKeys}>Retry</Button>
                </div>
              )}

              {!loading && !error && keys.length === 0 && (
                <div className="flex flex-1 flex-col items-center justify-center text-center">
                  <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-muted">
                    <Key className="h-5 w-5 text-muted-foreground" />
                  </div>
                  <p className="text-sm font-medium text-foreground">No API keys yet</p>
                  <p className="mt-1 text-xs text-muted-foreground">Create your first API key to get started.</p>
                </div>
              )}

              {!loading && !error && keys.length > 0 && (
                <motion.div
                  initial="initial"
                  animate="animate"
                  className="space-y-2"
                >
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
            </TabsContent>
          </Tabs>
        </DialogContent>
      </Dialog>

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
  );
}
