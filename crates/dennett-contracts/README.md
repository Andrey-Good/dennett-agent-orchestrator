# dennett-contracts

Общие типы, которые действительно пересекают process/protocol boundaries: stable IDs, envelopes, device role и deployment-role enums.

**Не помещать сюда:** provider-specific payloads, SQL rows, UI view models и все domain entities подряд.

Первый этап реализации:

1. синхронизировать shapes с Protobuf/JSON Schema;
2. добавить validation constructors;
3. закрепить compatibility fixtures;
4. сгенерировать отдельные wire DTO вместо прямой сериализации domain types там, где контракт станет публичным.
