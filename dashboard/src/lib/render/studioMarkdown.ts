import { marked } from 'marked';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
import sql from 'highlight.js/lib/languages/sql';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import go from 'highlight.js/lib/languages/go';
import DOMPurify from 'dompurify';

const RENDER_CACHE_MAX = 200;
const renderedHtmlCache = new Map<string, string>();

hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('py', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('rs', rust);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('shell', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('yml', yaml);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('go', go);

marked.use(
  markedHighlight({
    langPrefix: 'hljs language-',
    highlight(code: string, lang: string) {
      if (lang && hljs.getLanguage(lang)) {
        try {
          return hljs.highlight(code, { language: lang }).value;
        } catch {
          // Fall back to auto-detection below.
        }
      }
      try {
        return hljs.highlightAuto(code).value;
      } catch {
        return code;
      }
    },
  }),
);

marked.setOptions({ breaks: true, gfm: true });

function contentKey(content: string): string {
  let hash = 5381;
  for (let i = 0; i < content.length; i++) {
    hash = ((hash << 5) + hash + content.charCodeAt(i)) | 0;
  }
  return `${content.length}:${(hash >>> 0).toString(36)}`;
}

export function renderStudioMarkdown(content: string): string {
  if (!content) return '';

  const key = contentKey(content);
  const cached = renderedHtmlCache.get(key);
  if (cached !== undefined) {
    return cached;
  }

  const raw = marked.parse(content) as string;
  const sanitized = DOMPurify.sanitize(raw, {
    ALLOWED_TAGS: [
      'p', 'br', 'strong', 'em', 'code', 'pre', 'span', 'ul', 'ol', 'li',
      'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'a', 'blockquote', 'table',
      'thead', 'tbody', 'tr', 'th', 'td', 'hr', 'del', 'img',
      'details', 'summary', 'mark', 'kbd', 'sub', 'sup', 'figure', 'figcaption',
    ],
    ALLOWED_ATTR: ['class', 'href', 'target', 'rel', 'src', 'alt', 'title'],
  });

  if (renderedHtmlCache.size >= RENDER_CACHE_MAX) {
    const firstKey = renderedHtmlCache.keys().next().value;
    if (firstKey !== undefined) {
      renderedHtmlCache.delete(firstKey);
    }
  }
  renderedHtmlCache.set(key, sanitized);

  return sanitized;
}
