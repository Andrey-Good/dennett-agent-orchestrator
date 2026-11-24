import { useEffect, useRef, useState } from "react";
import {
  CaretDown,
  ChatsCircle,
  Check,
  FolderDashed,
  FolderSimple,
  SquaresFour,
} from "@phosphor-icons/react";

import type { ProjectRecord } from "../api/contracts";
import { PortalPopover } from "./PortalPopover";

export const STANDALONE_SCOPE_VALUE = "chat:standalone";

export interface ProjectScopeFilterProps {
  projects: ProjectRecord[];
  selectedProjectId: string | "all";
  onChange: (next: string | "all") => void;
  totalCount?: number;
  countByProjectId?: Map<string, number>;
  standaloneCount?: number;
  includeStandalone?: boolean;
  allLabel?: string;
  ariaLabel?: string;
}

export function ProjectScopeFilter({
  projects,
  selectedProjectId,
  onChange,
  totalCount,
  countByProjectId,
  standaloneCount,
  includeStandalone = false,
  allLabel = "Все проекты",
  ariaLabel = "Фильтр по проектам",
}: ProjectScopeFilterProps) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  // Auto-reset to "all" if the selected project was archived or removed.
  useEffect(() => {
    if (
      selectedProjectId !== "all" &&
      !(includeStandalone && selectedProjectId === STANDALONE_SCOPE_VALUE) &&
      !projects.some((p) => p.project_id === selectedProjectId)
    ) {
      onChange("all");
    }
  }, [includeStandalone, onChange, projects, selectedProjectId]);

  const isStandaloneSelected = includeStandalone && selectedProjectId === STANDALONE_SCOPE_VALUE;
  const selectedProject =
    selectedProjectId === "all" || isStandaloneSelected
      ? null
      : (projects.find((p) => p.project_id === selectedProjectId) ?? null);

  const labelText = isStandaloneSelected
    ? "Одиночные чаты"
    : selectedProject
      ? selectedProject.name
      : allLabel;

  return (
    <div className="project-scope-filter">
      <button
        ref={triggerRef}
        type="button"
        className={`project-scope-filter__trigger ${open ? "is-open" : ""}`}
        aria-label={ariaLabel}
        aria-expanded={open}
        aria-haspopup="menu"
        data-testid="project-scope-filter-trigger"
        onClick={() => setOpen((prev) => !prev)}
      >
        {isStandaloneSelected ? (
          <ChatsCircle size={14} aria-hidden="true" />
        ) : selectedProject ? (
          selectedProject.workspace_path ? (
            <FolderSimple size={14} aria-hidden="true" />
          ) : (
            <FolderDashed size={14} aria-hidden="true" />
          )
        ) : (
          <SquaresFour size={14} aria-hidden="true" />
        )}
        <span className="project-scope-filter__label">{labelText}</span>
        <CaretDown size={11} aria-hidden="true" />
      </button>

      <PortalPopover
        isOpen={open}
        anchorEl={triggerRef.current}
        onClose={() => setOpen(false)}
        placement="bottom-end"
        className="project-scope-filter__menu"
        role="menu"
        ariaLabel={ariaLabel}
        testId="project-scope-filter-menu"
      >
        <button
          type="button"
          role="menuitemradio"
          aria-checked={selectedProjectId === "all"}
          className={selectedProjectId === "all" ? "is-selected" : ""}
          data-testid="project-scope-option-all"
          onClick={() => {
            onChange("all");
            setOpen(false);
          }}
        >
          <span className="project-scope-filter__opt-copy">
            <SquaresFour size={14} aria-hidden="true" />
            <span>{allLabel}</span>
          </span>
          <span className="project-scope-filter__opt-meta">
            {totalCount !== undefined && (
              <small className="project-scope-filter__count">{totalCount}</small>
            )}
            {selectedProjectId === "all" && <Check size={13} aria-hidden="true" />}
          </span>
        </button>

        {includeStandalone && (
          <button
            type="button"
            role="menuitemradio"
            aria-checked={isStandaloneSelected}
            className={isStandaloneSelected ? "is-selected" : ""}
            data-testid="project-scope-option-standalone"
            onClick={() => {
              onChange(STANDALONE_SCOPE_VALUE);
              setOpen(false);
            }}
          >
            <span className="project-scope-filter__opt-copy">
              <ChatsCircle size={14} aria-hidden="true" />
              <span>Одиночные чаты</span>
            </span>
            <span className="project-scope-filter__opt-meta">
              {standaloneCount !== undefined && (
                <small className="project-scope-filter__count">{standaloneCount}</small>
              )}
              {isStandaloneSelected && <Check size={13} aria-hidden="true" />}
            </span>
          </button>
        )}

        {projects.length > 0 && <div className="popover-divider" role="separator" />}

        <div className="popover-scroll-list">
          {projects.map((project) => {
            const isSelected = selectedProjectId === project.project_id;
            const count = countByProjectId?.get(project.project_id);
            return (
              <button
                key={project.project_id}
                type="button"
                role="menuitemradio"
                aria-checked={isSelected}
                className={isSelected ? "is-selected" : ""}
                data-testid={`project-scope-option-${project.project_id}`}
                onClick={() => {
                  onChange(project.project_id);
                  setOpen(false);
                }}
              >
                <span className="project-scope-filter__opt-copy">
                  {project.workspace_path ? (
                    <FolderSimple size={14} aria-hidden="true" />
                  ) : (
                    <FolderDashed size={14} aria-hidden="true" />
                  )}
                  <span>{project.name}</span>
                </span>
                <span className="project-scope-filter__opt-meta">
                  {count !== undefined && (
                    <small className="project-scope-filter__count">{count}</small>
                  )}
                  {isSelected && <Check size={13} aria-hidden="true" />}
                </span>
              </button>
            );
          })}
        </div>
      </PortalPopover>
    </div>
  );
}
