import { describe, test, expect, vi, beforeEach, afterEach } from 'vitest';
import { Command } from 'commander';

// Prevent action handlers from calling process.exit
beforeEach(() => {
  vi.spyOn(process, 'exit').mockImplementation((() => { throw new Error('process.exit'); }) as never);
});
afterEach(() => {
  vi.restoreAllMocks();
});
import { registerIngestCommand } from '../../src/commands/ingest.js';
import { registerCompileCommand } from '../../src/commands/compile.js';
import { registerWriteCommand } from '../../src/commands/write.js';
import { registerSearchCommand } from '../../src/commands/search.js';
import { registerReadCommand } from '../../src/commands/read.js';
import { registerListCommand } from '../../src/commands/list.js';
import { registerWorkspacesCommand } from '../../src/commands/workspaces.js';
import { registerSubmitCommand } from '../../src/commands/submit.js';
import { registerReviewCommand } from '../../src/commands/review.js';
import { registerSetupCommand } from '../../src/commands/setup.js';
import { registerKeyCommand } from '../../src/commands/key.js';

function makeProgram() {
  const p = new Command()
    .name('cowiki')
    .option('--json')
    .option('-w, --workspace <slug>')
    .option('--server <url>')
    .exitOverride();
  return p;
}

function getCmd(program: Command, name: string): Command {
  const cmd = program.commands.find((c: Command) => c.name() === name);
  if (!cmd) throw new Error(`Command "${name}" not found`);
  return cmd;
}

/** Parse argv — Commander auto-strips first 2 (node, script) */
function parseArgs(p: Command, args: string[]) {
  p.parse(['node', 'cowiki', ...args]);
}

describe('command registration', () => {
  test('all 11 commands registered', () => {
    const p = makeProgram();
    registerSetupCommand(p);
    registerKeyCommand(p);
    registerIngestCommand(p);
    registerCompileCommand(p);
    registerWriteCommand(p);
    registerSearchCommand(p);
    registerReadCommand(p);
    registerListCommand(p);
    registerWorkspacesCommand(p);
    registerSubmitCommand(p);
    registerReviewCommand(p);

    const names = p.commands.map((c: Command) => c.name());
    expect(names.sort()).toEqual([
      'compile', 'ingest', 'key', 'list', 'read',
      'review', 'search', 'setup', 'submit', 'workspaces', 'write',
    ]);
  });
});

describe('ingest flags', () => {
  test('accepts --type and --content', () => {
    const p = makeProgram();
    registerIngestCommand(p);
    expect(() => parseArgs(p, ['ingest', '--type', 'text', '--content', 'hello'])).not.toThrow();
  });
});

describe('compile flags', () => {
  test('accepts --timeout', () => {
    const p = makeProgram();
    registerCompileCommand(p);
    expect(() => parseArgs(p, ['compile', '--timeout', '300'])).not.toThrow();
  });
});

describe('write flags', () => {
  test('requires slug argument', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() => parseArgs(p, ['write'])).toThrow();
  });

  test('accepts --title and --body', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() => parseArgs(p, ['write', 'my-page', '--title', 'Test', '--body', '# Hello'])).not.toThrow();
  });

  test('accepts --dir option', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() => parseArgs(p, ['write', 'my-page', '--dir', 'entities', '--body', '# Hi'])).not.toThrow();
  });

  test('accepts --dir with concepts', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() => parseArgs(p, ['write', 'my-page', '--dir', 'concepts', '--body', '# Hi'])).not.toThrow();
  });

  test('accepts nested slug with --dir', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() =>
      parseArgs(p, [
        'write', 'messy/hello',
        '--dir', 'wiki',
        '--title', 'hello',
        '--body', '# Hello',
      ])
    ).not.toThrow();
  });

  test('accepts deeply nested slug', () => {
    const p = makeProgram();
    registerWriteCommand(p);
    expect(() =>
      parseArgs(p, ['write', 'a/b/c/d/deep-page', '--dir', 'wiki', '--body', '# Deep'])
    ).not.toThrow();
  });
});

describe('search flags', () => {
  test('requires query argument', () => {
    const p = makeProgram();
    registerSearchCommand(p);
    expect(() => parseArgs(p, ['search'])).toThrow();
  });

  test('accepts --limit', () => {
    const p = makeProgram();
    registerSearchCommand(p);
    expect(() => parseArgs(p, ['search', 'test-query', '--limit', '20', '-w', 'testws'])).not.toThrow();
  });
});

describe('read flags', () => {
  test('requires slug argument', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() => parseArgs(p, ['read'])).toThrow();
  });

  test('accepts --no-pager', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() => parseArgs(p, ['read', 'some-page', '--no-pager'])).not.toThrow();
  });

  test('accepts --dir option', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() => parseArgs(p, ['read', 'some-page', '--dir', 'entities'])).not.toThrow();
  });

  test('accepts --dir with concepts', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() => parseArgs(p, ['read', 'some-page', '--dir', 'concepts'])).not.toThrow();
  });

  test('accepts nested slug with --dir', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() =>
      parseArgs(p, ['read', 'messy/mengnan-dating-guide', '--dir', 'wiki'])
    ).not.toThrow();
  });

  test('accepts deeply nested slug', () => {
    const p = makeProgram();
    registerReadCommand(p);
    expect(() =>
      parseArgs(p, ['read', 'a/b/c/d/deep-page', '--dir', 'wiki'])
    ).not.toThrow();
  });
});

describe('list flags', () => {
  test('accepts --dir option', () => {
    const p = makeProgram();
    registerListCommand(p);
    expect(() => parseArgs(p, ['list', '--dir', 'entities'])).not.toThrow();
  });

  test('accepts --dir all', () => {
    const p = makeProgram();
    registerListCommand(p);
    expect(() => parseArgs(p, ['list', '--dir', 'all'])).not.toThrow();
  });

  test('accepts --dir concepts', () => {
    const p = makeProgram();
    registerListCommand(p);
    expect(() => parseArgs(p, ['list', '--dir', 'concepts'])).not.toThrow();
  });
});

describe('submit flags', () => {
  test('accepts -y and --yes', () => {
    const p = makeProgram();
    registerSubmitCommand(p);
    expect(() => parseArgs(p, ['submit', '--yes', 'page1'])).not.toThrow();
    expect(() => parseArgs(p, ['submit', '-y', 'page1'])).not.toThrow();
  });

  test('accepts --all', () => {
    const p = makeProgram();
    registerSubmitCommand(p);
    expect(() => parseArgs(p, ['submit', '--all'])).not.toThrow();
  });
});

describe('review subcommands', () => {
  test('has list/show/approve/reject subcommands', () => {
    const p = makeProgram();
    registerReviewCommand(p);
    const review = getCmd(p, 'review');
    const subs = review.commands.map((c: Command) => c.name());
    expect(subs).toContain('list');
    expect(subs).toContain('show');
    expect(subs).toContain('approve');
    expect(subs).toContain('reject');
  });
});

describe('key subcommands', () => {
  test('has generate/list/revoke subcommands', () => {
    const p = makeProgram();
    registerKeyCommand(p);
    const key = getCmd(p, 'key');
    const subs = key.commands.map((c: Command) => c.name());
    expect(subs).toContain('generate');
    expect(subs).toContain('list');
    expect(subs).toContain('revoke');
  });

  test('generate requires --name', () => {
    const p = makeProgram();
    registerKeyCommand(p);
    expect(() => parseArgs(p, ['key', 'generate'])).toThrow();
  });
});
