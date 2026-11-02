import { useEffect, useState } from 'react';

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { C } from '@/lib/design';
import { PluginSettings } from '@/plugins/PluginSettings';
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
  onClose: () => void;
  onSave: (features: SpaceFeatures, preferences: PersonalPreferences) => Promise<void>;
}

export function SpaceFeaturesDialog({
  open,
  spaceName,
  features,
  preferences,
  plugins,
  ready,
  configurationError,
  onClose,
  onSave,
}: SpaceFeaturesDialogProps) {
  const [draft, setDraft] = useState(features);
  const [personal, setPersonal] = useState(preferences);
  const [saving, setSaving] = useState(false);
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

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose(); }}>
      <DialogContent style={{ maxWidth: 560, maxHeight: "88vh", overflowY: "auto" }}>
        <DialogHeader>
          <DialogTitle>Modules & settings</DialogTitle>
        </DialogHeader>
        <p style={{ margin: '-4px 0 6px', color: C.muted, fontSize: 12.5 }}>
          Space settings apply to everyone using <strong>{spaceName}</strong>. Turning a module off only hides its entry points; journal pages and tasks stay in your Markdown files.
        </p>

        {!ready && <p role="status" style={{ color: C.muted, fontSize: 12.5 }}>{configurationError || 'Loading Space settings…'}</p>}
        <fieldset disabled={!ready || saving} style={{ border: 0, padding: 0, margin: 0, display: 'contents' }}>
        <section style={sectionStyle}>
          <h3 style={headingStyle}>In this Space</h3>
          <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="toggles" />
          {MODULES.filter((module) => !plugins.some((plugin) => plugin.id === module.id)).map((module) => (
            <div key={module.id} style={moduleRow}>
              <label style={{ display: 'flex', gap: 10, alignItems: 'flex-start', flex: 1, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={draft.modules[module.id]}
                  onChange={(event) => setModule(module.id, event.target.checked)}
                  style={{ marginTop: 3 }}
                />
                <span>
                  <span style={{ display: 'block', fontSize: 13.5, fontWeight: 600, color: C.ink }}>{module.label}</span>
                  <span style={{ display: 'block', fontSize: 12, color: C.muted }}>{module.description}</span>
                </span>
              </label>
            </div>
          ))}
        </section>

        <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="fields" />

        <section style={sectionStyle}>
          <h3 style={headingStyle}>Templates</h3>
          <div style={{ display: 'grid', gridTemplateColumns: '120px 1fr', gap: '8px 12px', alignItems: 'center' }}>
            <label htmlFor="templates-folder" style={labelStyle}>Folder</label>
            <input
              id="templates-folder"
              value={draft.templates.folder}
              placeholder="Templates"
              onChange={(event) => setDraft((previous) => ({ ...previous, templates: { folder: event.target.value } }))}
              style={inputStyle}
            />
          </div>
          <p style={{ margin: '8px 0 0', fontSize: 11.5, color: C.muted }}>
            Every page in this folder is offered when you create a page. Templates are ordinary pages: edit or delete them like any note. They support <code>{'{{title}}'}</code>, <code>{'{{date}}'}</code> and <code>{'{{time}}'}</code>.
          </p>
        </section>

        <section style={sectionStyle}>
          <h3 style={headingStyle}>Only for me</h3>
          <PluginSettings plugins={plugins} features={draft} preferences={personal} onFeatures={setDraft} onPreferences={setPersonal} part="preferences" />
          <label style={{ display: 'flex', gap: 10, alignItems: 'flex-start', fontSize: 13, color: C.ink, cursor: 'pointer', marginTop: 8 }}>
            <input
              type="checkbox"
              checked={personal.autoCheckpoint}
              onChange={(event) => setPersonal((previous) => ({ ...previous, autoCheckpoint: event.target.checked }))}
              style={{ marginTop: 3 }}
            />
            <span>
              Save checkpoints automatically
              <span style={{ display: 'block', fontSize: 12, color: C.muted }}>
                After 5 minutes without changes, and when CoWiki quits. Checkpoints are kept in History; they never commit or share anything.
              </span>
            </span>
          </label>
        </section>
        </fieldset>

        {error && <p role="alert" style={{ color: C.red, fontSize: 12.5, margin: 0 }}>{error}</p>}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <Button variant="outline" onClick={onClose} disabled={saving}>Cancel</Button>
          <Button onClick={() => { void save(); }} disabled={saving || !ready}>{saving ? 'Saving…' : 'Save'}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

const sectionStyle: React.CSSProperties = {
  padding: '12px 14px', border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel,
};

const headingStyle: React.CSSProperties = {
  margin: '0 0 8px', fontSize: 11.5, fontWeight: 650, letterSpacing: '.04em', textTransform: 'uppercase', color: C.muted,
};

const moduleRow: React.CSSProperties = {
  display: 'flex', alignItems: 'flex-start', gap: 12, padding: '6px 0',
};

const labelStyle: React.CSSProperties = { fontSize: 12.5, color: C.ink2 };

const inputStyle: React.CSSProperties = {
  width: '100%', padding: '6px 9px', border: `1px solid ${C.line}`, borderRadius: 7,
  background: C.bg, color: C.ink, fontSize: 13,
};
