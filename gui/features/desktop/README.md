# @umbra/desktop

Foundation feature (tier `foundation`): the Tauri bridge (ADRs 0075, 0076). Today a stub: M8 WI-22 fills it.

## Public API
- `useEngineOutputSource(): JournalSource | undefined`: the engine process output as journal records (source `engine-process`, levels per ASM-827). `undefined` in the web build and until WI-22; `@umbra/log-view` shows the Engine process filter only when it is defined.
