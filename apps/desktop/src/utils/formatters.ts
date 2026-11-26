import type { ScheduleKind } from "../api/contracts";

/**
 * Strips raw Markdown backticks from plain-text system strings so technical tokens
 * never look like broken markdown inside status pills or cards.
 */
export function cleanPlainText(text: string): string {
  return text.replace(/`([^`]+)`/g, "$1");
}

/**
 * Formats a future UTC timestamp as a concise Russian human-readable countdown
 * (`через 45с`, `через 12м`, `через 2ч 15м`, `через 3д 4ч`).
 */
export function formatHumanCountdown(targetMs: number, nowMs: number): string {
  const diffSec = Math.max(0, Math.ceil((targetMs - nowMs) / 1000));
  if (diffSec === 0) {
    return "сейчас";
  }
  if (diffSec < 60) {
    return `через ${diffSec}с`;
  }
  const totalMins = Math.floor(diffSec / 60);
  if (totalMins < 60) {
    return `через ${totalMins}м`;
  }
  const totalHours = Math.floor(totalMins / 60);
  const remMins = totalMins % 60;
  if (totalHours < 24) {
    return `через ${totalHours}ч ${remMins}м`;
  }
  const days = Math.floor(totalHours / 24);
  const remHours = totalHours % 24;
  return `через ${days}д ${remHours}ч`;
}

const ISO_WEEKDAY_SHORT_RU = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] as const;

export function formatScheduleSummaryRu(kind: ScheduleKind, timezone: string): string {
  switch (kind.kind) {
    case "once_at": {
      const dt = new Date(kind.fire_at);
      return dt.toLocaleString("ru-RU", {
        day: "2-digit",
        month: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      });
    }
    case "interval": {
      const secs = kind.interval_secs;
      if (secs < 60) {
        return `каждые ${secs}с`;
      }
      if (secs < 3600) {
        return `каждые ${Math.round(secs / 60)}м`;
      }
      const hours = Math.floor(secs / 3600);
      const remMins = Math.round((secs % 3600) / 60);
      if (remMins > 0 && hours < 24) {
        return `каждые ${hours}ч ${remMins}м`;
      }
      if (hours >= 24 && hours % 24 === 0) {
        const days = hours / 24;
        return days === 1 ? "каждый день" : `каждые ${days}д`;
      }
      return `каждые ${Math.round(secs / 3600)}ч`;
    }
    case "daily_wall_time": {
      const hh = String(kind.hour).padStart(2, "0");
      const mm = String(kind.minute).padStart(2, "0");
      return `ежедневно в ${hh}:${mm} (${timezone})`;
    }
    case "weekly_wall_time": {
      const days: string[] = [];
      for (let bit = 0; bit < 7; bit += 1) {
        if ((kind.weekdays_mask & (1 << bit)) !== 0) {
          days.push(ISO_WEEKDAY_SHORT_RU[bit]!);
        }
      }
      const hh = String(kind.hour).padStart(2, "0");
      const mm = String(kind.minute).padStart(2, "0");
      return `${days.join(", ")} в ${hh}:${mm} (${timezone})`;
    }
  }
}

/**
 * Translates common English backend policy / approval / error reasons into natural Russian
 * without hiding actionable details (file paths, tool names, or custom user notes).
 */
export function formatDecisionReasonRu(reason: string | null | undefined): string {
  if (!reason) {
    return "";
  }
  const cleaned = cleanPlainText(reason.trim());
  if (!cleaned) {
    return "";
  }

  if (/workspace_write_policy\s*=\s*ask_before_write/i.test(cleaned)) {
    return "Включён режим «Спрашивать запись» — требуется подтверждение владельца перед изменением файлов.";
  }
  if (/workspace_write_policy\s*=\s*deny_all/i.test(cleaned)) {
    return "Запись файлов запрещена политикой проекта («Только чтение»).";
  }
  if (/default-deny outside model/i.test(cleaned)) {
    return "Действие отклонено внешней политикой безопасности (нет разрешающего правила).";
  }
  if (/project is no longer active/i.test(cleaned)) {
    return "Действие отклонено: проект переведён в архив или удалён.";
  }
  if (/outside project workspace|escapes workspace|path traversal/i.test(cleaned)) {
    return "Доступ запрещён: путь выходит за пределы рабочей папки проекта.";
  }
  if (/owner decision:\s*approve_once/i.test(cleaned)) {
    return "Разрешено владельцем однократно.";
  }
  if (/owner decision:\s*approve_project/i.test(cleaned)) {
    return "Разрешено владельцем для проекта.";
  }
  if (/owner decision:\s*approve_task/i.test(cleaned)) {
    return "Разрешено владельцем для поручения.";
  }
  if (/owner decision:\s*deny/i.test(cleaned)) {
    return "Отклонено владельцем.";
  }
  if (/lost acknowledgement/i.test(cleaned)) {
    return "Подтверждение от внешнего сервиса потеряно (UNKNOWN).";
  }
  if (/permission request expired|approval timed out/i.test(cleaned)) {
    return "Время ожидания подтверждения истекло.";
  }
  if (/cancelled by owner|run cancelled/i.test(cleaned)) {
    return "Остановлено владельцем.";
  }
  if (/budget exceeded|exceeded budget/i.test(cleaned)) {
    return "Достигнут внешний лимит бюджета поручения.";
  }
  if (/reconciled: confirmed/i.test(cleaned)) {
    return "Сверка завершена: сервис подтвердил выполнение — дубль не отправлялся.";
  }
  if (/reconciled: not executed|marked not executed|reconciled: failed/i.test(cleaned)) {
    return "Сверка завершена: отмечено невыполненным — повтор безопасен.";
  }
  return cleaned;
}

/**
 * Formats token counts into a compact human-readable representation (`850`, `12.7K`, `1.0M`).
 */
export function formatTokenCount(tokens: number): string {
  if (!Number.isFinite(tokens) || tokens < 0) {
    return "0";
  }
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`;
  }
  if (tokens >= 1_000) {
    return `${(tokens / 1_000).toFixed(1)}K`;
  }
  return String(Math.round(tokens));
}

/**
 * Formats live or persisted run context usage for `StatsPopover` (e.g. `12.7K / 1.0M` tokens).
 */
export function formatContextUsage(
  usedTokens: number | null | undefined,
  windowTokens: number | null | undefined,
): string | null {
  if (usedTokens == null && windowTokens == null) {
    return null;
  }
  if (usedTokens != null && windowTokens != null && windowTokens > 0) {
    return `${formatTokenCount(usedTokens)} / ${formatTokenCount(windowTokens)}`;
  }
  if (usedTokens != null) {
    return `${formatTokenCount(usedTokens)} токенов`;
  }
  if (windowTokens != null && windowTokens > 0) {
    return `окно ${formatTokenCount(windowTokens)}`;
  }
  return null;
}
