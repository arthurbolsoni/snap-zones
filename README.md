# snap-zones

Encaixe de janelas para monitor ultrawide no Windows. Fica na bandeja do sistema.

Arrastando uma janela pela barra de título, nada aparece no meio da tela. Encostando o cursor numa borda do monitor aparece só a zona daquela borda:

| Borda | Zona |
| --- | --- |
| esquerda | metade esquerda |
| direita | metade direita |
| cima | tela cheia (ocupa a área toda sem maximizar, continua arrastável) |

Segurando `Alt` durante o arraste aparecem todas as zonas, e a posição do cursor escolhe:

| Zona | Área | Quando aparece |
| --- | --- | --- |
| 1, 2, 3 (azul) | terços | sempre |
| A (laranja) | metade da largura, centrada (25%–75%) | as duas metades ocupadas |
| Tela cheia | notch no alto, ao centro | sempre |

- Zonas sobrepostas: vence a de centro mais próximo do cursor; empate entre A e terço 2 fica com a A.
- Zona ocupada = janela visível encaixada nela, no mesmo monitor e na área de trabalho virtual atual.
- `Ctrl` ao soltar (com `Alt`): ocupa duas colunas vizinhas.
- `Shift` durante o arraste: move sem encaixar.
- Janela encaixada (inclusive em tela cheia) volta ao tamanho de antes do encaixe assim que começa a ser puxada; o arraste continua com ela já no tamanho original.
- Redimensionar uma janela pela borda lateral leva junto a vizinha que divide a mesma borda (50x50 → 25x75).
- Menu do ícone: Pausar, Sair.

Janelas de programas rodando como administrador só encaixam se o snap-zones também rodar como administrador.

## Build

```
cargo build --release
```

Executável em `target/release/snap-zones.exe`.

`snap-zones --preview <pasta>` desenha os estados do overlay e o ícone em BMP, sem abrir nada na tela.
