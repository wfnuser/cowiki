/**
 * Which view a URL stands for. Used to follow browser/WebView back and
 * forward, which change the URL without running any click handler.
 */
import { parseReviewRoute, type ReviewTarget } from '../components/review/review-navigation.ts';

export type RouteView =
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; target: ReviewTarget }
  | { kind: 'page'; workspaceSlug: string; conceptId: string };

export function viewForRoute(pathname: string, workspaceSlugs: string[]): RouteView | null {
  const review = parseReviewRoute(pathname, workspaceSlugs);
  if (review) {
    return review.target
      ? { kind: 'review-detail', workspaceSlug: review.workspaceSlug, target: review.target }
      : { kind: 'review-list', workspaceSlug: review.workspaceSlug };
  }
  const parts = pathname.split('/').filter(Boolean);
  if (parts.length < 2) return null;
  try {
    const [space, ...page] = parts.map((part) => decodeURIComponent(part));
    if (!workspaceSlugs.includes(space)) return null;
    return { kind: 'page', workspaceSlug: space, conceptId: page.join('/') };
  } catch {
    return null;
  }
}

type ShownView =
  | { kind: 'review-list'; workspaceSlug: string }
  | { kind: 'review-detail'; workspaceSlug: string; target: ReviewTarget }
  | { kind: 'page'; slug: string }
  | { kind: string }
  | null;

/** Whether the view on screen already is the one the URL stands for. */
export function showsRoute(shown: ShownView, shownSpace: string | null, route: RouteView): boolean {
  if (!shown || shown.kind !== route.kind || shownSpace !== route.workspaceSlug) return false;
  if (route.kind === 'page') return (shown as { slug: string }).slug === route.conceptId;
  if (route.kind === 'review-detail') {
    return JSON.stringify((shown as { target: ReviewTarget }).target) === JSON.stringify(route.target);
  }
  return true;
}
