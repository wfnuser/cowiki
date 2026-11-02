# Trusted built-in plugins

Daily Notes is the first built-in contribution. Tasks, Flashcards and drawing
remain existing application features; this change does not migrate them.

## The boundary

`web/src/plugins/builtins.ts` is the only production list. Its entries are
reviewed TypeScript shipped with CoWiki, **not installable third-party plugins**.
CoWiki does not discover, import or evaluate JavaScript from a Space. There is
no marketplace, external-code loading, permission UI or sandbox promise here.
This is an internal application boundary, not an isolation boundary against
malicious code bundled into the application.

The host registers namespaced action IDs, rejects duplicate plugin/action IDs,
lists actions only for a local Space whose configuration has loaded, and
rechecks current enablement when a command is invoked. Pending/failed settings
are not default-enabled settings. Launch hooks run at most once, after initial
navigation/configuration, only for a root launch and an explicit device-local
preference. Page/review deep links are not replaced.

An action receives only the current Space's page adapter: read a Markdown page,
create a page **only if it does not exist**, refresh the tree, and open through
ordinary page navigation. No overwrite, deletion, arbitrary filesystem access
or cross-Space parameter is exposed. The adapter rejects absolute, traversal,
hidden and reserved document paths. Compatible colon-bearing filenames/date
formats remain usable. Native missing-file and already-exists errors are
recognized narrowly; other read/write errors surface to the user. Switching
Spaces or changing settings invalidates pending actions before their next
operation or navigation. An already issued create-only operation cannot be
undone, but cannot overwrite an existing note or navigate the new Space.

## Adding another built-in

Create a trusted definition and append it to `BUILTIN_PLUGINS`. For example:

```ts
import type { BuiltinPlugin } from './host';

export const examplePlugin: BuiltinPlugin = {
  id: 'example',
  label: 'Example',
  description: 'Open an ordinary example note.',
  enabled: (features) => features.modules.example !== false,
  setEnabled: (features, enabled) => ({
    ...features,
    modules: { ...features.modules, example: enabled },
  }),
  actions: [{
    id: 'open',
    label: 'Example note',
    icon: 'file',
    run: async ({ pages }) => {
      if (await pages.read('Example.md') === null) {
        await pages.createOnly('Example.md', '# Example\n');
        await pages.refresh();
      }
      await pages.open('Example.md');
    },
  }],
};
```

The resulting `example:open` sidebar action needs no `MainLayout` switch case,
sidebar branch or dialog branch. Boolean `features.modules.example` settings
round-trip without adding the ID to the legacy `ModuleId` union. The example is
documentation/test material only; production registers Daily Notes alone.

Optional `settings` contribute text field readers/updaters, a preview/help text
and device-only preference readers/updaters to the existing dialog. Optional
`launch` names an action and a preference predicate. Daily Notes demonstrates
both in `daily-notes.ts`. A new persisted content/device setting needs its typed
parser/default/storage contract; the shell still does not render plugin-owned
fields itself. Add behavior tests, including a second definition through the
real host and UI, before registering a new built-in.

## Existing Spaces and notes

No migration or setting rename is required. Daily Notes continues using
`features.modules.journal` and `features.journal.{folder,format,template}` in
`.cowiki/space.json`, with the existing defaults and Obsidian-compatible date
and template conventions. It opens the same Markdown path each day. Creating
a missing date page is create-only, including a concurrent-create race; an
existing page is never rewritten by Today.

Disabling Daily Notes removes Today and the launch behavior. It does **not**
delete/convert files, hide their tree entries, or prevent ordinary Markdown
editing. Tasks' existing “add to today” uses the shared date-page file helper
without invoking or enabling the plugin. A missing optional template falls
back to the usual journal body; an unreadable template is an error, not absence.

`openJournalOnStartup` remains in `cowiki.personal.preferences` on the device,
not in the Space. Saving settings rereads the current Space config and applies
only fields edited against the dialog's base. Fresh unrelated values (including
other plugin flags), unknown keys and omitted defaults stay as they are. A
competing edit to the same field requires reloading, and a device-only preference
change does not rewrite the Space file.

Missing settings use defaults; malformed JSON, a nonobject JSON root or an
unreadable file is an error, not a new Space. That failure disables plugin
actions/settings writes without locking ordinary Wiki access or changing the
file's bytes. After repairing the file externally, use **Reload settings** in
the dialog to reread it. Turning a local plugin on/off adds no Cloud/team
feature behavior.

## Verification

From `web`, run the focused tests and then the existing gates:

```sh
node --experimental-strip-types --test tests/daily-plugin.test.ts tests/journal.test.ts tests/space-features.test.ts
npm test
npm run lint
npm run build
```

The Daily Notes suite uses disposable real filesystem stores, native-IPC error
shapes, held reads, atomic create-only races, public config saving and actual
rendered sidebar/settings components. Desktop-shell browser acceptance and
the independent native draft-read performance fix are separate checks.
