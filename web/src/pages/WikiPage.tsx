import { SearchBar } from '../components/SearchBar';
import { PageList } from '../components/PageList';

export function WikiPage() {
  return (
    <div>
      <h1 className="font-[var(--font-serif)] text-4xl font-bold text-[var(--color-text)] mb-1" style={{ fontFamily: 'var(--font-serif)' }}>
        Shared Wiki
      </h1>
      <p className="text-[var(--color-text-secondary)] text-sm mb-6">
        Knowledge maintained by the team and their agents.
      </p>
      <div className="mb-6">
        <SearchBar />
      </div>
      <PageList branch="main" />
    </div>
  );
}
