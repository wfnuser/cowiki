import type { PersonalPreferences, SpaceFeatures } from '../lib/space-features';
import { fonts } from '../lib/design';
import { useI18n } from '../lib/I18nContext';
import type { BuiltinPlugin } from './host';

interface Props {
  plugins: readonly BuiltinPlugin[];
  features: SpaceFeatures;
  preferences: PersonalPreferences;
  onFeatures: (features: SpaceFeatures) => void;
  onPreferences: (preferences: PersonalPreferences) => void;
  part?: 'toggles' | 'fields' | 'preferences';
}

/** One setting per row: its name and one short line, the switch at the end. */
export function SettingSwitch({ id, label, description, checked, disabled, onChange }: {
  id?: string;
  label: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-4 py-2.5 has-[:disabled]:cursor-default">
      <span className="min-w-0 flex-1">
        <span className="block text-small font-medium leading-5 text-text">{label}</span>
        {description && <span className="block text-meta leading-[1.45] text-text-tertiary">{description}</span>}
      </span>
      <input id={id} type="checkbox" className="cw-switch" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
    </label>
  );
}

export const settingsInputClass = 'h-8 w-full min-w-0 rounded-lg border border-border bg-panel px-2.5 text-small text-text outline-none transition-colors placeholder:text-text-faint hover:border-border-hover focus-visible:border-accent/60 focus-visible:shadow-[var(--shadow-focus)] focus-visible:outline-none disabled:opacity-50';

/** The dialog knows how to render settings, not which fields a built-in owns. */
export function PluginSettings({ plugins, features, preferences, onFeatures, onPreferences, part }: Props) {
  const { t, tl } = useI18n();
  return plugins.map((plugin) => (
    <div key={plugin.id} className="contents">
      {(!part || part === 'toggles') && (
        <SettingSwitch
          label={tl(plugin.label)}
          description={tl(plugin.description)}
          checked={plugin.enabled(features)}
          onChange={(enabled) => onFeatures(plugin.setEnabled(features, enabled))}
        />
      )}
      {plugin.settings && (!part || part === 'fields') && (
        <section className={part === 'fields' ? 'settings-section is-first' : 'settings-section'} aria-disabled={!plugin.enabled(features)}>
          <h3 className={part === 'fields' ? 'sr-only' : 'settings-heading'}>{tl(plugin.settings.title)}</h3>
          {part === 'fields' && !plugin.enabled(features) && (
            <p className="mb-3 flex items-center justify-between gap-3 text-meta text-text-tertiary">
              <span>{t('{label} is off for this Space.', { label: tl(plugin.label) })}</span>
              <button
                type="button"
                onClick={() => onFeatures(plugin.setEnabled(features, true))}
                className="shrink-0 font-medium text-accent hover:underline"
              >
                {t('Turn on')}
              </button>
            </p>
          )}
          <div className="grid grid-cols-[112px_minmax(0,1fr)] items-center gap-x-3 gap-y-2" style={{ opacity: plugin.enabled(features) ? 1 : 0.55 }}>
            {plugin.settings.fields.map((field) => (
              <div key={field.id} className="contents">
                <label htmlFor={field.id} className="text-small text-text-secondary">{tl(field.label)}</label>
                <input
                  id={field.id}
                  className={settingsInputClass}
                  value={field.value(features)}
                  placeholder={field.placeholder ? tl(field.placeholder) : undefined}
                  disabled={!plugin.enabled(features)}
                  onChange={(event) => onFeatures(field.update(features, event.target.value))}
                  style={field.monospace ? { fontFamily: fonts.mono, fontSize: 13 } : undefined}
                />
              </div>
            ))}
          </div>
          <p className="mt-2.5 text-meta leading-[1.55] text-text-tertiary">
            {plugin.settings.preview && (
              <span className="mb-0.5 block">
                {tl(plugin.settings.preview.label)}{' '}
                <code className="rounded-[4px] bg-bg-secondary px-1 py-px text-text-secondary" style={{ fontFamily: fonts.mono }}>
                  {plugin.settings.preview.value(features, new Date())}
                </code>
              </span>
            )}
            {tl(plugin.settings.help)}
          </p>
        </section>
      )}
      {(!part || part === 'preferences') && plugin.settings?.preferences?.map((setting) => (
        <SettingSwitch
          key={setting.id}
          id={setting.id}
          label={tl(setting.label)}
          checked={setting.value(preferences)}
          onChange={(value) => onPreferences(setting.update(preferences, value))}
        />
      ))}
    </div>
  ));
}
