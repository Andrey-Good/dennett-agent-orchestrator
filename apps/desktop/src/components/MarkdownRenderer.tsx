import { isValidElement, useCallback, useState, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "@phosphor-icons/react";

export interface MarkdownRendererProps {
  content: string;
  className?: string;
}

const ALLOWED_EXTERNAL_SCHEMES = new Set(["http:", "https:"]);

/**
 * Validates that a URL uses strictly `http:` or `https:`.
 * Blocks `javascript:`, `data:`, `vbscript:`, `file:`, and malformed/relative navigation targets.
 */
export function sanitizeExternalUrl(rawHref: string | undefined | null): string | null {
  if (!rawHref) {
    return null;
  }
  const trimmed = rawHref.trim();
  if (!trimmed) {
    return null;
  }
  try {
    const parsed = new URL(trimmed);
    if (!ALLOWED_EXTERNAL_SCHEMES.has(parsed.protocol.toLowerCase())) {
      return null;
    }
    return parsed.toString();
  } catch {
    return null;
  }
}

/**
 * Opens a validated external `http:`/`https:` URL without ever navigating the Tauri webview.
 */
export function openSafeExternalUrl(rawHref: string | undefined | null): boolean {
  const safeUrl = sanitizeExternalUrl(rawHref);
  if (!safeUrl || typeof window === "undefined") {
    return false;
  }
  window.open(safeUrl, "_blank", "noopener,noreferrer");
  return true;
}

function extractPlainText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map(extractPlainText).join("");
  }
  if (isValidElement<{ children?: ReactNode }>(node)) {
    return extractPlainText(node.props.children);
  }
  return "";
}

function extractCodeLanguage(node: ReactNode): string | null {
  if (isValidElement<{ className?: string }>(node)) {
    const cls = node.props.className ?? "";
    const match = /language-([a-zA-Z0-9_+-]+)/.exec(cls);
    if (match?.[1]) {
      return match[1];
    }
  }
  return null;
}

function CodeBlockPre({ children }: { children?: ReactNode }) {
  const [copied, setCopied] = useState(false);
  const rawCode = extractPlainText(children).replace(/\n$/, "");
  const language = extractCodeLanguage(children);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(rawCode);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // Clipboard API may be unavailable in restricted contexts.
    }
  }, [rawCode]);

  return (
    <div className="md-code-block" data-testid="markdown-code-block">
      <div className="md-code-block__header">
        <span className="md-code-block__lang">{language ?? "код"}</span>
        <button
          type="button"
          className="md-code-block__copy-btn"
          data-testid="code-block-copy-btn"
          aria-label="Копировать код"
          title="Копировать код"
          onClick={() => {
            void handleCopy();
          }}
        >
          {copied ? <Check size={12} aria-hidden="true" /> : <Copy size={12} aria-hidden="true" />}
          <span>{copied ? "Скопировано" : "Копировать"}</span>
        </button>
      </div>
      <pre className="md-code-block__pre">{children}</pre>
    </div>
  );
}

const MARKDOWN_COMPONENTS: Components = {
  pre: ({ children }) => <CodeBlockPre>{children}</CodeBlockPre>,
  a: ({ href, children }) => {
    const safeHref = sanitizeExternalUrl(href);
    if (!safeHref) {
      return (
        <span
          className="md-blocked-link"
          data-testid="blocked-external-link"
          title="Небезопасная ссылка заблокирована"
        >
          {children}
        </span>
      );
    }
    return (
      <a
        href={safeHref}
        target="_blank"
        rel="noopener noreferrer"
        className="md-external-link"
        data-testid="safe-external-link"
        onClick={(event) => {
          event.preventDefault();
          openSafeExternalUrl(safeHref);
        }}
      >
        {children}
      </a>
    );
  },
};

const REMARK_PLUGINS = [remarkGfm];

export function MarkdownRenderer({ content, className }: MarkdownRendererProps) {
  return (
    <div className={className ? `md-body ${className}` : "md-body"}>
      <ReactMarkdown
        remarkPlugins={REMARK_PLUGINS}
        skipHtml
        components={MARKDOWN_COMPONENTS}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
