# snap-zones

Encaixe de janelas para monitor ultrawide no Windows. Fica na bandeja do sistema.

Arrastando uma janela pela barra de título, nada aparece no meio da tela. Encostando o cursor numa borda do monitor aparece só a zona daquela borda:

| Borda | Zona |
| --- | --- |
| esquerda | metade esquerda |
| direita | metade direita |
| cima | maximiza |

Segurando `Alt` durante o arraste aparecem todas as zonas, e a posição do cursor escolhe:

| Zona | Área | Quando aparece |
| --- | --- | --- |
| 1, 2, 3 | terços | sempre |
| A | largura de um terço, centrada na divisa 1/2 (17%–50%) | 1 e 2 ocupados |
| B | largura de um terço, centrada na divisa 2/3 (50%–83%) | 2 e 3 ocupados |
| Tela cheia | notch no alto, ao centro; maximiza | sempre |

- Zonas sobrepostas: vence a de centro mais próximo do cursor; A e B ficam por cima dos terços.
- Zona ocupada = janela visível encaixada nela, no mesmo monitor e na área de trabalho virtual atual.
- `Ctrl` ao soltar (com `Alt`): ocupa duas colunas vizinhas.
- `Shift` durante o arraste: move sem encaixar.
- Janela maximizada (pelo app ou pelo botão) pode ser arrastada pela barra de título: volta ao tamanho de antes de maximizar, como no snap do Windows.
- Janela encaixada volta ao tamanho de antes do encaixe assim que começa a ser puxada; o arraste continua com ela já no tamanho original.
- Redimensionar uma janela pela borda lateral leva junto a vizinha que divide a mesma borda (50x50 → 25x75).
- Menu do ícone: Pausar, Sair.

Janelas de programas rodando como administrador só encaixam se o snap-zones também rodar como administrador.

## Build

```
cargo build --release
```

Executável em `target/release/snap-zones.exe`.

`snap-zones --preview <pasta>` desenha os estados do overlay e o ícone em BMP, sem abrir nada na tela.
