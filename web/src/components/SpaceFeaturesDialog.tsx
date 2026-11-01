import { useEffect, useState } from 'react';

import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { C, fonts } from '@/lib/design';
import { journalPagePath } from '@/lib/journal';
import {
  MODULES,
  setModuleHidden,
  type ModuleId,
  type PersonalPreferences,
  type SpaceFeatures,
} from '@/lib/space-features';

interface SpaceFeaturesDialogProps {
  open: boolean;
  spaceId: string;
  spaceName: string;
  features: SpaceFeatures;
  preferences: PersonalPreferences;
  onClose: () => void;
  onSave: (features: SpaceFeatures, preferences: PersonalPreferences) => Promise<void>;
  /** Write the starter templates into a folder; resolves to how many were added. */
  onAddStarterTemplates: (folder: string) => Promise<number>;
}

export function SpaceFeaturesDialog({
  open,
  spaceId,
  spaceName,
  features,
  preferences,
  onClose,
  onSave,
  onAddStarterTemplates,
}: SpaceFeaturesDialogProps) {
  const [draft, setDraft] = useState(features);
  const [personal, setPersonal] = useState(preferences);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [starterNote, setStarterNote] = useState('');

  useEffect(() => {
    if (open) {
      setDraft(features);
      setPersonal(preferences);
      setError('');
      setStarterNote('');
    }
  }, [open, features, preferences]);


  const hidden = new Set(personal.hiddenModules[spaceId] ?? []);
  const preview = (() => {
    try {
      return journalPagePath(draft.journal, new Date());
    } catch {
      return '';
    }
  })();

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

        <section style={sectionStyle}>
          <h3 style={headingStyle}>In this Space</h3>
          {MODULES.map((module) => (
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
              <label style={{ display: 'inline-flex', gap: 6, alignItems: 'center', fontSize: 12, color: C.muted, whiteSpace: 'nowrap' }}>
                <input
                  type="checkbox"
                  checked={hidden.has(module.id)}
                  disabled={!draft.modules[module.id]}
                  onChange={(event) => setPersonal((previous) => setModuleHidden(previous, spaceId, module.id, event.target.checked))}
                />
                Hide for me
              </label>
            </div>
          ))}
        </section>

        <section style={{ ...sectionStyle, opacity: draft.modules.journal ? 1 : 0.55 }}>
          <h3 style={headingStyle}>Journal</h3>
          <div style={{ display: 'grid', gridTemplateColumns: '120px 1fr', gap: '8px 12px', alignItems: 'center' }}>
            <label htmlFor="journal-folder" style={labelStyle}>Folder</label>
            <input
              id="journal-folder"
              value={draft.journal.folder}
              placeholder="Space root"
              disabled={!draft.modules.journal}
              onChange={(event) => setDraft((previous) => ({ ...previous, journal: { ...previous.journal, folder: event.target.value } }))}
              style={inputStyle}
            />
            <label htmlFor="journal-format" style={labelStyle}>Date format</label>
            <input
              id="journal-format"
              value={draft.journal.format}
              disabled={!draft.modules.journal}
              onChange={(event) => setDraft((previous) => ({ ...previous, journal: { ...previous.journal, format: event.target.value } }))}
              style={{ ...inputStyle, fontFamily: fonts.mono }}
            />
            <label htmlFor="journal-template" style={labelStyle}>Template page</label>
            <input
              id="journal-template"
              value={draft.journal.template ?? ''}
              placeholder="Optional, e.g. Templates/Daily.md"
              disabled={!draft.modules.journal}
              onChange={(event) => setDraft((previous) => ({
                ...previous,
                journal: { ...previous.journal, template: event.target.value.trim() ? event.target.value : null },
              }))}
              style={inputStyle}
            />
          </div>
          <p style={{ margin: '8px 0 0', fontSize: 11.5, color: C.muted }}>
            Today’s entry: <code style={{ fontFamily: fonts.mono }}>{preview || '—'}</code>. Formats use Obsidian/moment tokens such as <code>YYYY-MM-DD</code> or <code>YYYY/MM/YYYY-MM-DD</code>; templates support <code>{'{{date}}'}</code>, <code>{'{{title}}'}</code> and <code>{'{{time}}'}</code>.
          </p>
        </section>

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
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 8 }}>
            <Button
              variant="outline"
              type="button"
              onClick={() => {
                const folder = draft.templates.folder.trim() || 'Templates';
                onAddStarterTemplates(folder)
                  .then((added) => setStarterNote(added > 0 ? `Added ${added} templates to ${folder}/.` : `${folder}/ already has them.`))
                  .catch((caught: unknown) => setStarterNote(caught instanceof Error ? caught.message : String(caught)));
              }}
            >
              Add starter templates
            </Button>
            <span style={{ fontSize: 11.5, color: C.muted }}>{starterNote || 'Learning plan, Practice log, Meeting notes.'}</span>
          </div>
        </section>

        <section style={sectionStyle}>
          <h3 style={headingStyle}>Only for me</h3>
          <label style={{ display: 'flex', gap: 10, alignItems: 'center', fontSize: 13, color: C.ink, cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={personal.openJournalOnStartup}
              onChange={(event) => setPersonal((previous) => ({ ...previous, openJournalOnStartup: event.target.checked }))}
            />
            Open today’s journal when CoWiki starts
          </label>
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

        {error && <p role="alert" style={{ color: C.red, fontSize: 12.5, margin: 0 }}>{error}</p>}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <Button variant="outline" onClick={onClose} disabled={saving}>Cancel</Button>
          <Button onClick={() => { void save(); }} disabled={saving}>{saving ? 'Saving…' : 'Save'}</Button>
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
