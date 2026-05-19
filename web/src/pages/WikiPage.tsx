import { SearchBar } from '../components/SearchBar';
import { PageList } from '../components/PageList';

export function WikiPage() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-stone-800">Shared Wiki</h1>
      </div>
      <SearchBar />
      <PageList branch="main" />
    </div>
  );
}
