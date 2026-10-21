# snap-zones

Encaixe de janelas para monitor ultrawide no Windows. Fica na bandeja do sistema.

Ao arrastar uma janela pela barra de título, as zonas aparecem sobre o monitor:

| Zona | Área | Quando aparece |
| --- | --- | --- |
| 1, 2, 3 | colunas de um terço da tela | sempre |
| A | largura de uma coluna, centrada na divisa 1/2 | 1 e 2 ocupadas, ou B ocupada |
| B | largura de uma coluna, centrada na divisa 2/3 | 2 e 3 ocupadas, ou A ocupada |
| Tela cheia | notch no alto, ao centro | sempre; soltar maximiza |

- Zonas sobrepostas: vence a de centro mais próximo do cursor.
- Zona ocupada = janela visível encaixada nela, no mesmo monitor e na área de trabalho virtual atual.
- `Ctrl` ao soltar: ocupa as duas colunas vizinhas.
- `Shift` durante o arraste: move sem encaixar.
- Menu do ícone: Pausar, Sair.

Janelas de programas rodando como administrador só encaixam se o snap-zones também rodar como administrador.

## Build

```
cargo build --release
```

Executável em `target/release/snap-zones.exe`.
