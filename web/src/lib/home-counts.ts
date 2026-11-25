import { listBrokenLinks, listCardSources, listTaskSources, readCardReviews } from '../local-api.ts';
import { buildQueue, collectCards, parseReviewStore } from './flashcards.ts';
import { dateKey, journalDateFromPath, type JournalConfig } from './journal.ts';
import { extractTasks, groupTasks } from './tasks.ts';

/** What Home asks you to do today; null where the module is off. */
export interface HomeCounts {
  /** Open tasks due today or overdue (the Tasks view's Today). */
  tasks: number | null;
  /** Cards in today's review queue (the Flashcards view's Review button). */
  cards: number | null;
  /** Links to pages that do not exist (the Broken links view); local Spaces only. */
  links: number | null;
}

export async function loadHomeCounts(options: {
  spaceSlug: string;
  journal: JournalConfig;
  tasks: boolean;
  cards: boolean;
  links?: boolean;
  now?: Date;
}): Promise<HomeCounts> {
  const now = options.now ?? new Date();
  const today = dateKey(now);
  const [tasks, cards, links] = await Promise.all([
    options.tasks
      ? listTaskSources(options.spaceSlug).then((sources) => {
        const all = sources.flatMap((source) => {
          const day = journalDateFromPath(options.journal, source.path);
          return extractTasks(source.path, source.content, day ? dateKey(day) : null);
        });
        const groups = groupTasks(all, today, 0);
        return groups.overdue.length + groups.today.length;
      })
      : null,
    options.cards
      ? Promise.all([listCardSources(options.spaceSlug), readCardReviews(options.spaceSlug)])
        .then(([sources, saved]) => buildQueue(collectCards(sources), parseReviewStore(saved), now, today).length)
      : null,
    options.links ? listBrokenLinks(options.spaceSlug).then((found) => found.length) : null,
  ]);
  return { tasks, cards, links };
}
