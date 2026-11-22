import { useEffect, useState } from 'react';

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { PluginSettings, SettingSwitch, settingsInputClass } from '@/plugins/PluginSettings';
import type { BuiltinPlugin } from '@/plugins/host';
import {
  MODULES,
  type ModuleId,
  type PersonalPreferences,
  type SpaceFeatures,
} from '@/lib/space-features';

interface SpaceFeaturesDialogProps {
  open: boolean;
  spaceName: string;
  features: SpaceFeatures;
  preferences: PersonalPreferences;
  plugins: readonly BuiltinPlugin[];
  ready: boolean;
  configurationError?: string;
  onReload: () => Promise<void>;
  onClose: () => void;
  onSave: (features: SpaceFeatures, preferences: PersonalPreferences) => Promise<void>;
}

/** A long form: centered, title and actions pinned, the body scrolls; it
 * never extends past the window (DESIGN_SYSTEM.md, "Dialogs"). */
export function SpaceFeaturesDialog({
  open,
  spaceName,
  features,
  preferences,
  plugins,
  ready,
  configurationError,
  onReload,
  onClose,
  onSave,
}: SpaceFeaturesDialogProps) {
  const [draft, setDraft] = useState(features);
  const [personal, setPersonal] = useState(preferences);
  const [saving, setSaving] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (open) {
      setDraft(features);
      setPersonal(preferences);
      setError('');
    }
  }, [open, features, preferences]);

  const setModule = (id: ModuleId, enabled: boolean) => {
    setDraft((previous) => ({ ...previous, modules: { ...previous.modules, [id]: enabled } }));
  };

  const save = async () => {
    setSaving(true);
    setError('');
    try {
      await onSave(draft, personal);
      onClose();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSaving(false);
    }
  };

  const reload = async () => {
    setReloading(true);
    setError('');
    try { await onReload(); }
    catch (caught) { setError(caught instanceof Error ? caught.message : String(caught)); }
    finally { setReloading(false); }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose(); }}>
      <DialogContent className="top-1/2 flex max-h-[min(760px,calc(100vh-3rem))] -translate-y-1/2 flex-col gap-0 overflow-hidden p-0 sm:max-w-[540px]">
        <DialogHeader className="shrink-0 gap-1 px-6 pb-3 pt-5 pr-12">
          <DialogTitle className="text-[16px] font-semibold leading-6">Modules & settings</DialogTitle>
          <DialogDescription className="text-[12.5px] leading-[1.5] text-text-tertiary">
            Space settings apply to everyone in <span className="font-medium text-text-secondary">{spaceName}</span>. Turning a module off only hides it; your notes stay.
          </DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-2">
          {!ready && <p role="status" className="pb-2 pt-1 text-[12.5px] text-text-tertiary">{configurationError || 'Loading Space settings…'}</p>}
          <fieldset disabled={!ready || saving || reloading} style={{ border: 0, padding: 0, margin: 0, display: 'contents' }}>
            <section className="settings-section is-first">
              <h3 className="settings-heading mb-0">Modules</h3>
              <div>
                <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="toggles" />
                {MODULES.filter((module) => !plugins.some((plugin) => plugin.id === module.id)).map((module) => (
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

            <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="fields" />

            <section className="settings-section">
              <h3 className="settings-heading">Templates</h3>
              <div className="grid grid-cols-[112px_minmax(0,1fr)] items-center gap-x-3">
                <label htmlFor="templates-folder" className="text-[12.5px] text-text-secondary">Folder</label>
                <input
                  id="templates-folder"
                  className={settingsInputClass}
                  value={draft.templates.folder}
                  placeholder="Templates"
                  onChange={(event) => setDraft((previous) => ({ ...previous, templates: { folder: event.target.value } }))}
                />
              </div>
              <p className="mt-2.5 text-[11.5px] leading-[1.55] text-text-tertiary">
                Pages in this folder are offered when you create a page. They support <code className="font-mono">{'{{title}}'}</code>, <code className="font-mono">{'{{date}}'}</code> and <code className="font-mono">{'{{time}}'}</code>.
              </p>
            </section>

            <section className="settings-section">
              <h3 className="settings-heading mb-0">Only for me</h3>
              <div>
                <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="preferences" />
                <SettingSwitch
                  label="Save checkpoints automatically"
                  description="After 5 minutes without changes, and when CoWiki quits. Kept in History; never committed or shared."
                  checked={personal.autoCheckpoint}
                  onChange={(autoCheckpoint) => setPersonal((previous) => ({ ...previous, autoCheckpoint }))}
                />
              </div>
            </section>
          </fieldset>
        </div>

        <div className="flex shrink-0 items-center gap-2 border-t border-border-soft bg-panel px-6 py-3">
          {error
            ? <p role="alert" className="min-w-0 flex-1 truncate text-[12.5px] text-red" title={error}>{error}</p>
            : <span className="flex-1" />}
          {(configurationError || error) && <Button size="sm" variant="outline" onClick={() => { void reload(); }} disabled={saving || reloading}>{reloading ? 'Reloading…' : 'Reload settings'}</Button>}
          <Button size="sm" variant="outline" onClick={onClose} disabled={saving || reloading}>Cancel</Button>
          <Button size="sm" onClick={() => { void save(); }} disabled={saving || reloading || !ready}>{saving ? 'Saving…' : 'Save'}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
