# dennett-head

Единственный логический control plane активной установки. Композирует Agentic, Trust, Memory, Events, Sync, Effects и Resource coordination через ports.

Head не обязан быть отдельным физическим сервером. ПК может запускать этот профиль только после явного разрешения пользователя и установки canonical data role.

В M01 `SessionCoordinator` является единственным писателем project-session journal: admission сначала фиксируется, затем подтверждается и публикуется как monotonic watch delta. Подписка всегда начинается с snapshot; lag/gap возвращает `ResyncRequired`, а не пытается угадать пропущенное состояние.
