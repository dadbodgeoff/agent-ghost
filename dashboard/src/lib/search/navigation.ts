import type { SearchResult } from '@ghost/sdk';

export function hrefForSearchResult(result: SearchResult, fallbackQuery?: string): string {
  if (result.navigation?.href) {
    return result.navigation.href;
  }

  const encodedQuery = fallbackQuery ? `?q=${encodeURIComponent(fallbackQuery)}` : '';
  switch (result.result_type) {
    case 'agent':
      return `/agents/${result.id}`;
    case 'session':
      return `/sessions/${result.id}`;
    case 'proposal':
      return `/goals/${result.id}`;
    case 'memory':
      return `/memory/${result.id}${encodedQuery}`;
    case 'audit':
      return `/security${fallbackQuery ? `?search=${encodeURIComponent(fallbackQuery)}&focus=${encodeURIComponent(result.id)}` : `?focus=${encodeURIComponent(result.id)}`}`;
    default:
      return `/search${encodedQuery}`;
  }
}
