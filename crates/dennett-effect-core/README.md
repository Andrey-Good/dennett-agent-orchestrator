# dennett-effect-core

Жизненный цикл consequential external effects: prepare, dispatch, confirmed/failed/unknown, reconciliation и compensation.

Ни connector, ни agent adapter не должны выполнять send/pay/publish/delete в обход этого модуля.
