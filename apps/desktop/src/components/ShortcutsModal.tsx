import { useCallback, useRef } from "react";
import { Keyboard, X } from "@phosphor-icons/react";

import { useWorkspace } from "../state/WorkspaceProvider";
import { useModalFocusTrap } from "./useModalFocusTrap";

const SHORTCUT_ITEMS: Array<{ keys: string; description: string }> = [
  { keys: "Ctrl + K", description: "Командная палитра и быстрый переход" },
  { keys: "Ctrl + F", description: "Поиск по сообщениям в текущем чате" },
  { keys: "Ctrl + ,", description: "Открыть или закрыть Настройки" },
  { keys: "Ctrl + N", description: "Создать новый чат" },
  { keys: "Ctrl + B", description: "Показать или скрыть боковую панель проектов" },
  { keys: "Ctrl + \\", description: "Показать или скрыть панель файлов и артефактов" },
  { keys: "Alt + 1 / 2 / 3 / 4", description: "Переключение: Чат, Входящие, Поручения, Память" },
  { keys: "Enter / Shift + Enter", description: "Отправить сообщение / перенос строки (настраивается)" },
  { keys: "Ctrl + Enter", description: "Прервать активный ход и уточнить сейчас (или отправка)" },
  { keys: "Esc", description: "Закрыть диалог или остановить активный запуск" },
];

export function ShortcutsModal() {
  const { state, actions } = useWorkspace();
  const dialogRef = useRef<HTMLDivElement | null>(null);

  const close = useCallback(() => {
    actions.setShortcutsOpen(false);
  }, [actions]);

  useModalFocusTrap(dialogRef, state.isShortcutsOpen, close);

  if (!state.isShortcutsOpen) {
    return null;
  }

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={close}
      data-testid="shortcuts-modal-backdrop"
    >
      <div
        ref={dialogRef}
        className="floating-dialog shortcuts-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Горячие клавиши"
        data-testid="shortcuts-modal"
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="dialog-header">
          <div className="dialog-title-row">
            <Keyboard size={16} aria-hidden="true" />
            <strong>Горячие клавиши</strong>
          </div>
          <button
            type="button"
            className="icon-button icon-button--xs"
            onClick={close}
            aria-label="Закрыть справку по горячим клавишам"
          >
            <X size={14} />
          </button>
        </div>

        <ul className="shortcuts-list">
          {SHORTCUT_ITEMS.map((item) => (
            <li key={item.keys} className="shortcuts-list__row">
              <span>{item.description}</span>
              <kbd>{item.keys}</kbd>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
