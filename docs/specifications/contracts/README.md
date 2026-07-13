# Интегрированные cross-domain contracts

Эти файлы являются нормативным разнесением бывшего временного предархитектурного документа. Они нужны потому, что некоторые жизненные циклы пересекают Memory, Agentic, Trust, Capability, Server и UI. Копирование полного правила во все большие документы создало бы конфликтующие дубли.

Каждый supplement объявляет primary owner. Другие документы применяют контракт и описывают свою часть исполнения или интерфейса, не переопределяя смысл.

| ID | Contract | Primary owner |
|---|---|---|
| 00 | [Shared cross-domain rules](00_Shared_Cross_Domain_Rules.md) | Specification Index |
| A | [Ambient sensory capture](A_Ambient_Sensory_Capture_Contract.md) | Voice / Server |
| B | [External communication operation](B_External_Communication_Operation.md) | Agentic Control |
| C | [Project lifecycle](C_Project_Lifecycle_Contract.md) | Agentic Control |
| D | [Artifact lifecycle](D_Artifact_Lifecycle_Contract.md) | Agentic Control |
| E | [Update, compatibility and migration](E_Update_Compatibility_and_Migration_Contract.md) | Server Runtime |
| F | [Identity, key and ownership recovery](F_Identity_Key_and_Ownership_Recovery_Contract.md) | Trust / Server |
| G | [Resource pressure and usage accounting](G_Resource_Pressure_and_Usage_Accounting_Contract.md) | Server Runtime |
| H | [Federated global search](H_Federated_Global_Search_Contract.md) | Server Runtime / Memory adapters |
| I | [Locale, timezone, language and travel](I_Locale_Timezone_Language_and_Travel_Contract.md) | Server Runtime |
| J | [Import/export and portable packages](J_Import_Export_and_Portable_Package_Compatibility_Contract.md) | Capability / Server |
| K | [Composite experience recipes](K_Composite_Experience_Recipes.md) | Agentic / Capability |
| 90 | [Integrated scenarios and risk spikes](90_Integrated_End_to_End_Scenarios.md) | End-to-End Validation |

Исторический исходник сохранён в `docs/archive/distributed/` только для provenance и сравнения diff; он не является вторым источником истины.
