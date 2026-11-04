import type { PersonalPreferences, SpaceFeatures } from '../lib/space-features';
import { C, fonts } from '../lib/design';
import type { BuiltinPlugin } from './host';

interface Props {
  plugins: readonly BuiltinPlugin[];
  features: SpaceFeatures;
  preferences: PersonalPreferences;
  onFeatures: (features: SpaceFeatures) => void;
  onPreferences: (preferences: PersonalPreferences) => void;
  part?: 'toggles' | 'fields' | 'preferences';
}

/** The dialog knows how to render settings, not which fields a built-in owns. */
export function PluginSettings({ plugins, features, preferences, onFeatures, onPreferences, part }: Props) {
  return plugins.map((plugin) => (
    <div key={plugin.id}>
      {(!part || part === 'toggles') && (
        <label style={{ display: 'flex', gap: 10, alignItems: 'flex-start', padding: '6px 0', cursor: 'pointer' }}>
          <input type="checkbox" checked={plugin.enabled(features)} style={{ marginTop: 3 }}
            onChange={(event) => onFeatures(plugin.setEnabled(features, event.target.checked))} />
          <span>
            <span style={{ display: 'block', fontSize: 13.5, fontWeight: 600, color: C.ink }}>{plugin.label}</span>
            <span style={{ display: 'block', fontSize: 12, color: C.muted }}>{plugin.description}</span>
          </span>
        </label>
      )}
      {plugin.settings && (!part || part === 'fields') && (
        <section style={{ padding: '12px 14px', border: `1px solid ${C.line}`, borderRadius: 10, background: C.panel, opacity: plugin.enabled(features) ? 1 : 0.55 }}>
          <h3 style={{ margin: '0 0 8px', fontSize: 11.5, fontWeight: 650, letterSpacing: '.04em', textTransform: 'uppercase', color: C.muted }}>{plugin.settings.title}</h3>
          <div style={{ display: 'grid', gridTemplateColumns: '120px 1fr', gap: '8px 12px', alignItems: 'center' }}>
            {plugin.settings.fields.map((field) => (
              <div key={field.id} style={{ display: 'contents' }}>
                <label htmlFor={field.id} style={{ fontSize: 12.5, color: C.ink2 }}>{field.label}</label>
                <input id={field.id} value={field.value(features)} placeholder={field.placeholder} disabled={!plugin.enabled(features)}
                  onChange={(event) => onFeatures(field.update(features, event.target.value))}
                  style={{ width: '100%', padding: '6px 9px', border: `1px solid ${C.line}`, borderRadius: 7, background: C.bg, color: C.ink, fontSize: 13,
                    fontFamily: field.monospace ? fonts.mono : undefined }} />
              </div>
            ))}
          </div>
          <p style={{ margin: '8px 0 0', fontSize: 11.5, color: C.muted }}>
            {plugin.settings.preview && <><code style={{ fontFamily: fonts.mono }}>{plugin.settings.preview(features, new Date())}</code>. </>}
            {plugin.settings.help}
          </p>
        </section>
      )}
      {(!part || part === 'preferences') && plugin.settings?.preferences?.map((setting) => (
        <label key={setting.id} style={{ display: 'flex', gap: 10, alignItems: 'center', fontSize: 13, color: C.ink, cursor: 'pointer' }}>
          <input id={setting.id} type="checkbox" checked={setting.value(preferences)}
            onChange={(event) => onPreferences(setting.update(preferences, event.target.checked))} />
          {setting.label}
        </label>
      ))}
    </div>
  ));
}
