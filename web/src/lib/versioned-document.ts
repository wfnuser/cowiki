export type DocumentWriter = 'human' | 'agent';

export interface DocumentSnapshot {
  content: string;
  revision: number;
  writer: DocumentWriter | null;
}

export interface DocumentReplacement {
  content: string;
  expectedRevision: number;
  writer: DocumentWriter;
}

/**
 * Raised internally when an edit was prepared from an obsolete document.
 * The UI should not present this as a failure: agents use `latest` to
 * regenerate their patch, matching VS Code's optimistic document model.
 */
export class StaleDocumentError extends Error {
  readonly latest: DocumentSnapshot;

  constructor(latest: DocumentSnapshot) {
    super(`document changed (latest revision: ${latest.revision})`);
    this.name = 'StaleDocumentError';
    this.latest = latest;
  }
}

/** In-memory authority for one open editor document. */
export class VersionedDocument {
  private current: DocumentSnapshot;

  constructor(initialContent: string, initialRevision = 0) {
    this.current = {
      content: initialContent,
      revision: initialRevision,
      writer: null,
    };
  }

  snapshot(): DocumentSnapshot {
    return { ...this.current };
  }

  replace(edit: DocumentReplacement): DocumentSnapshot {
    if (edit.expectedRevision !== this.current.revision) {
      throw new StaleDocumentError(this.snapshot());
    }

    if (edit.content === this.current.content) return this.snapshot();

    this.current = {
      content: edit.content,
      revision: this.current.revision + 1,
      writer: edit.writer,
    };
    return this.snapshot();
  }
}

export interface AgentEditPort {
  read: () => Promise<DocumentSnapshot>;
  propose: (latest: DocumentSnapshot) => Promise<string>;
  write: (edit: DocumentReplacement) => Promise<DocumentSnapshot>;
}

/**
 * Apply an agent edit optimistically. A concurrent human edit invalidates the
 * proposal; the agent then reads the newest text and writes a fresh proposal.
 */
export async function applyAgentEditWithRetry(
  port: AgentEditPort,
  maxAttempts = 4,
): Promise<DocumentSnapshot> {
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const base = await port.read();
    const content = await port.propose(base);
    try {
      return await port.write({
        content,
        expectedRevision: base.revision,
        writer: 'agent',
      });
    } catch (error) {
      if (!(error instanceof StaleDocumentError) || attempt === maxAttempts) throw error;
    }
  }

  throw new Error('agent edit retry loop exhausted');
}
