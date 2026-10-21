# snap-zones

Encaixe de janelas para monitor ultrawide no Windows. Fica na bandeja do sistema.

Arrastando uma janela pela barra de título, nada aparece no meio da tela. Encostando o cursor numa borda do monitor aparece só a zona daquela borda:

| Borda | Zona |
| --- | --- |
| esquerda | metade esquerda, ou todo o espaço que sobra ao lado de uma janela presa à direita |
| direita | metade direita, ou todo o espaço que sobra ao lado de uma janela presa à esquerda |
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
- Redimensionar pela borda de cima até o topo da tela (ou pela de baixo até o fim) deixa a janela com a altura toda, mantendo largura e posição; arrastar depois volta à altura de antes.
- Redimensionar uma janela pela borda lateral leva junto a vizinha que divide a mesma borda (50x50 → 25x75).
- Janela em tela cheia (jogo, vídeo, F11) fica fora: o app só age em janela em modo janela.
- Menu do ícone: Pausar, Iniciar com o Windows (valor `snap-zones` em `HKCUSoftwareMicrosoftWindowsCurrentVersionRun`), Sair.

Janelas de programas rodando como administrador só encaixam se o snap-zones também rodar como administrador.

## Build

```
cargo build --release
```

Executável em `target/release/snap-zones.exe`.

`snap-zones --preview <pasta>` desenha os estados do overlay e o ícone em BMP, sem abrir nada na tela.
