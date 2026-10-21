# snap-zones

Encaixe de janelas para monitor ultrawide no Windows. Fica na bandeja do sistema.

Ao arrastar uma janela pela barra de título, as zonas aparecem sobre o monitor:

| Zona | Área | Quando aparece |
| --- | --- | --- |
| 1, 2 | metades da tela (padrão) | sempre |
| A | metade da largura, centrada na divisa (25%–75%) | 1 e 2 ocupadas |
| 1, 2, 3 | terços da tela | segurando `Alt` |
| Tela cheia | notch no alto, ao centro | sempre; soltar maximiza |

- Zonas sobrepostas: vence a de centro mais próximo do cursor.
- Zona ocupada = janela visível encaixada nela, no mesmo monitor e na área de trabalho virtual atual.
- `Ctrl` ao soltar (com `Alt`): ocupa duas colunas vizinhas.
- `Shift` durante o arraste: move sem encaixar.
- Redimensionar uma janela pela borda lateral leva junto a vizinha que divide a mesma borda (50x50 → 25x75).
- Menu do ícone: Pausar, Sair.

Janelas de programas rodando como administrador só encaixam se o snap-zones também rodar como administrador.

## Build

```
cargo build --release
```

Executável em `target/release/snap-zones.exe`.
