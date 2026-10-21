# Testes de arraste (e2e)

Simulam o mouse de verdade sobre uma janela de teste sempre no topo (`form.ps1`), com o
snap-zones rodando. Movem o cursor por alguns segundos: não mexa no mouse durante o teste.

- `drag-test.ps1`: janela maximizada puxada pela barra de título volta a 900x600, segue o
  cursor e para ao soltar.
- `snap-test.ps1`: janela encaixada na borda esquerda, puxada rápido, volta a 900x600, segue o
  cursor e para ao soltar.

```
pwsh -NoProfile -File tests/e2e/drag-test.ps1
pwsh -NoProfile -File tests/e2e/snap-test.ps1
```

Com `SNAPZONES_LOG=1` no ambiente do snap-zones, o gancho de mouse registra em
`%TEMP%\snap-zones.log`.
