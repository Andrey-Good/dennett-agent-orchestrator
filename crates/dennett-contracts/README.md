# dennett-contracts

Общие типы, которые действительно пересекают process/protocol boundaries: stable IDs, envelopes, device role и deployment-role enums.

**Не помещать сюда:** provider-specific payloads, SQL rows, UI view models и все domain entities подряд.

Первый этап реализации:

1. синхронизировать shapes с Protobuf/JSON Schema;
2. добавить validation constructors;
3. закрепить compatibility fixtures;
4. сгенерировать отдельные wire DTO вместо прямой сериализации domain types там, где контракт станет публичным.

## M02 project/workspace boundary

- `ProjectId` остаётся логической идентичностью и не выводится из пути.
- `WorkspaceBindingId` обозначает локальную связь проекта с местом работы.
- `WorkspaceRevision` связывает binding, ненулевую монотонную последовательность и snapshot ID.
- `ProjectRelativePath` — только канонический относительный путь; проверка здесь отсекает лексические escape-формы, но не заменяет handle-relative защиту Workspace Manager от symlink/junction races.
- `.dennett` может переносить идентичность и общую project memory, но не локальные permissions, чаты, secrets или personal memory.
- Project files and model output never grant `ProjectTrustState`.
- `UNSPECIFIED` при регистрации нормализуется в `Restricted`, но запрещён для изменения trust; повышение прав дополнительно требует Trust-issued decision reference.
- Копия с уже используемым portable ID разрешается явно: сохранить тот же логический проект и его локальную policy или зарегистрировать fork с новым ID; rebind никогда не создаёт новый проект.
- Общие validators запрещают противоречивые состояния operation, command и test receipts до публикации через Node.
- Отмена, проигравшая гонку уже завершённой операции, сохраняет terminal receipt неизменным и завершается как idempotent no-op.

Полные transport DTO для inspection, registration, changes, command/test receipts,
artifacts, checkpoints и review генерируются из `protocols/proto`; доменный crate
не дублирует их и не содержит filesystem, Git, provider или UI types.
