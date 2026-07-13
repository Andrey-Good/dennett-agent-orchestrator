# denet-memoryd

Необязательный process wrapper вокруг `denet-memory-core`. Нужен для reuse, independent indexing lifecycle и fault/resource isolation.

`InProcessMemoryAdapter` и `MemoryServiceAdapter` обязаны проходить одинаковые golden/conformance tests.
