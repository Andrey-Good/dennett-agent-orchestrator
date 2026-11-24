import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";

export type PopoverPlacement = "bottom-start" | "bottom-end" | "top-start" | "top-end";

export interface PortalPopoverProps {
  isOpen: boolean;
  anchorEl: HTMLElement | null;
  onClose: () => void;
  placement?: PopoverPlacement;
  offset?: number;
  matchAnchorWidth?: boolean;
  className?: string;
  role?: string;
  ariaLabel: string;
  testId?: string;
  children: ReactNode;
}

// Global mutual-exclusion registry: opening any PortalPopover automatically closes all others.
const openPopoverClosers = new Map<symbol, () => void>();

function registerOpenPopover(token: symbol, closeFn: () => void): void {
  for (const [otherToken, otherClose] of Array.from(openPopoverClosers.entries())) {
    if (otherToken !== token) {
      openPopoverClosers.delete(otherToken);
      otherClose();
    }
  }
  openPopoverClosers.set(token, closeFn);
}

function unregisterOpenPopover(token: symbol): void {
  openPopoverClosers.delete(token);
}

export function PortalPopover({
  isOpen,
  anchorEl,
  onClose,
  placement = "bottom-end",
  offset = 6,
  matchAnchorWidth = false,
  className = "",
  role = "menu",
  ariaLabel,
  testId,
  children,
}: PortalPopoverProps) {
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const tokenRef = useRef<symbol | null>(null);
  if (!tokenRef.current) {
    tokenRef.current = Symbol("portal-popover");
  }
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  const [coords, setCoords] = useState<{
    top: number;
    left: number;
    flippedUp: boolean;
    width?: number;
  } | null>(null);

  // Enforce mutual exclusion across all active portaled menus.
  useEffect(() => {
    const token = tokenRef.current!;
    if (!isOpen) {
      unregisterOpenPopover(token);
      return;
    }
    registerOpenPopover(token, () => onCloseRef.current());
    return () => {
      unregisterOpenPopover(token);
    };
  }, [isOpen]);

  // Compute viewport-clamped fixed coordinates relative to anchorEl.
  useLayoutEffect(() => {
    if (!isOpen || !anchorEl) {
      setCoords(null);
      return;
    }

    const computePosition = () => {
      const pop = popoverRef.current;
      const anchorRect = anchorEl.getBoundingClientRect();
      const vw = window.innerWidth || 1280;
      const vh = window.innerHeight || 800;
      const margin = 8;

      const popRect = pop?.getBoundingClientRect();
      const popWidth = popRect && popRect.width > 0 ? popRect.width : 230;
      const popHeight = popRect && popRect.height > 0 ? popRect.height : 180;

      const preferTop = placement.startsWith("top");
      const alignEnd = placement.endsWith("end");

      const spaceBelow = vh - anchorRect.bottom - offset - margin;
      const spaceAbove = anchorRect.top - offset - margin;
      const hasAnchorGeometry =
        anchorRect.width > 0 || anchorRect.height > 0 || anchorRect.top > 0;

      let flippedUp = preferTop;
      if (hasAnchorGeometry) {
        if (!preferTop && spaceBelow < popHeight && spaceAbove > spaceBelow) {
          flippedUp = true;
        } else if (preferTop && spaceAbove < popHeight && spaceBelow > spaceAbove) {
          flippedUp = false;
        }
      }

      let rawTop = flippedUp
        ? anchorRect.top - popHeight - offset
        : anchorRect.bottom + offset;
      let rawLeft = alignEnd ? anchorRect.right - popWidth : anchorRect.left;

      // Clamp strictly within viewport margins.
      const maxLeft = Math.max(margin, vw - popWidth - margin);
      const maxTop = Math.max(margin, vh - popHeight - margin);
      rawLeft = Math.min(Math.max(margin, rawLeft), maxLeft);
      rawTop = Math.min(Math.max(margin, rawTop), maxTop);

      setCoords({
        top: Math.round(rawTop),
        left: Math.round(rawLeft),
        flippedUp,
        width: matchAnchorWidth && anchorRect.width > 0 ? Math.round(anchorRect.width) : undefined,
      });
    };

    computePosition();
    const rafId = window.requestAnimationFrame(computePosition);
    window.addEventListener("resize", computePosition);

    const handleScroll = (event: Event) => {
      const target = event.target as Node | null;
      if (target && popoverRef.current?.contains(target)) {
        return;
      }
      computePosition();
    };
    window.addEventListener("scroll", handleScroll, { capture: true, passive: true });

    return () => {
      window.cancelAnimationFrame(rafId);
      window.removeEventListener("resize", computePosition);
      window.removeEventListener("scroll", handleScroll, { capture: true });
    };
  }, [anchorEl, isOpen, matchAnchorWidth, offset, placement]);

  // Dismiss on outside mousedown or capture-phase Escape (stopImmediatePropagation prevents run Stop!).
  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const handleMouseDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) {
        return;
      }
      if (popoverRef.current?.contains(target) || anchorEl?.contains(target)) {
        return;
      }
      onCloseRef.current();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        onCloseRef.current();
      }
    };

    document.addEventListener("mousedown", handleMouseDown);
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    return () => {
      document.removeEventListener("mousedown", handleMouseDown);
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [anchorEl, isOpen]);

  if (!isOpen || typeof document === "undefined") {
    return null;
  }

  const style: CSSProperties = coords
    ? {
        position: "fixed",
        top: `${coords.top}px`,
        left: `${coords.left}px`,
        minWidth: coords.width ? `${coords.width}px` : undefined,
        zIndex: 80,
      }
    : {
        position: "fixed",
        top: "8px",
        left: "8px",
        zIndex: 80,
        visibility: "hidden",
      };

  return createPortal(
    <div
      ref={popoverRef}
      className={`floating-popover portal-popover ${coords?.flippedUp ? "is-flipped-up" : ""} ${className}`.trim()}
      style={style}
      role={role}
      aria-label={ariaLabel}
      data-testid={testId}
      data-placement={coords?.flippedUp ? "top" : "bottom"}
      onMouseDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
    >
      {children}
    </div>,
    document.body,
  );
}
