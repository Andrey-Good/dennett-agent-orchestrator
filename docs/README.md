# Карта документации

Общий статус и приоритет: [restart/README.md](restart/README.md). Начало реализации: [implementation/STATUS.md](implementation/STATUS.md). В документационной отправной точке кода и исполнимых тестов нет.

## Что строим

[Функциональная концепция](specifications/00_Dennett_Functional_Concept.md), [общие контракты](specifications/01_Dennett_Specification_Index_and_Shared_Contracts.md), предметные спецификации 10–70 и [дополнения](specifications/contracts/README.md). Согласованные поправки — [основания](restart/01_FOUNDATIONS.md), [проверяемое самоулучшение](restart/02_CHANGE_EVALUATION.md), [представление L](specifications/contracts/L_Presentation_and_Interaction_Contract.md).

## Как устроено

[Архитектура 80–83](architecture/README.md), [карта будущего кода](architecture/CODE_MAP.md), [проверка согласованности V01–V29](architecture/ARCHITECTURE_VALIDATION.md), [решения](decisions/README.md). Архитектура задаёт границы, не доказывает работающую реализацию.

## Как реализуем и принимаем

[Стратегия](implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md), [процесс агента](implementation/01_AGENT_EXECUTION_PROTOCOL.md), [владелец](implementation/02_OWNER_PLAYBOOK.md), [задачи](implementation/03_WORK_PACKAGE_SYSTEM.md), [D0–D9](implementation/04_MILESTONE_DEPENDENCY_MAP.md), [тесты](testing/TEST_CATALOGUE_AND_QUALITY_GATES.md), [покрытие](testing/REQUIREMENTS_COVERAGE.md), [статус](implementation/STATUS.md).

## Материалы

[Визуальное направление](design/README.md) сохраняется, реализация экранов пишется заново. [Исследования разработки](research/2026-09-27_development_evidence.md) содержат первичные источники и ограничения. Старые архитектурные тексты и мысли в [archive](archive/README.md) являются историей, не новым заданием.

Команды запуска/проверок создаются в D0 и попадают в [development runbook](runbooks/development.md). Не выдумывать работоспособность отсутствующих старых команд и не возвращать их код автоматически. Старые generated catalogues и planning-статусы не являются доказательством готовности.
