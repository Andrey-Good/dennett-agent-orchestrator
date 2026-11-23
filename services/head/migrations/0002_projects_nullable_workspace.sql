-- Allow folderless project creation ("Пока без папки") with later Attach/Rebind Folder via PATCH /v1/projects/{project_id}.
ALTER TABLE projects ALTER COLUMN workspace_path DROP NOT NULL;
ALTER TABLE projects DROP CONSTRAINT IF EXISTS projects_workspace_path_check;
ALTER TABLE projects ADD CONSTRAINT projects_workspace_path_check
    CHECK (workspace_path IS NULL OR (octet_length(workspace_path) > 0 AND octet_length(workspace_path) <= 1024));
