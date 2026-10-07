# Como a Neko funciona

> Guia em português, focado em entender o fluxo. O resto de `docs/` está em
> inglês por consistência do projeto.

## A ideia em uma frase

A Neko é um **despachante**: você escreve `neko.alguma_coisa(...)`, o core
traduz para uma chamada numa **interface abstrata** (`Backend`), e o backend
escolhido na compilação (SDL2 no desktop, gsKit no PS2) executa de verdade.

Você nunca fala com SDL. Você fala com `neko.*`.

## As três peças

```
   SEU JOGO                CORE (neko.*)              BACKEND
  (ex.: struct Game)      src/neko.zig + core/       sdl2/ ou ps2/
        │                        │                        │
        │  neko.draw.rect(...)   │                        │
        └───────────────────────►│  context.get()         │
                                 │  e.draw_rect(...)      │
                                 └───────────────────────►│  SDL_RenderFillRect
                                                          │  (ou gsKit no PS2)
```

- **Seu jogo** conhece só `neko.*`.
- **O core** não conhece sistema operacional nenhum; ele só chama a interface.
- **O backend** é quem faz o trabalho pesado (janela, GPU, áudio, teclado, arquivos).

## O que acontece na compilação

No `build.zig` são criados dois módulos:

- `neko` → `src/neko.zig` (o core).
- `neko_backend` → `src/backends/sdl2/sdl2.zig` **ou** `src/backends/ps2/ps2.zig`,
  conforme `.backend = .sdl2` / `.ps2`.

O core importa o backend por baixo (em `src/core/platform.zig`), mas o seu jogo só
importa `neko`:

```zig
const neko = @import("neko"); // resolve para src/neko.zig
```

É por isso que **trocar o backend não muda o seu jogo**: o arquivo que vira
`neko_backend` muda; o `@import("neko")` continua o mesmo.

## O que acontece no runtime (o exemplo `app.zig`)

1. O Zig chama `pub fn main(init: std.process.Init)`.
   - `init.gpa` é o alocador; `init.io` é o I/O do processo.
2. `var game = Game{}` — o **seu estado**, criado na stack (x, y, vx, vy…).
3. `neko.app.run(Game, &game, config)` faz:
   - **`screen.init(config)`**
     - guarda `allocator` e `assets_dir` no `context`;
     - `platform.create()` devolve o backend (ex.: o `Sdl2Engine`);
     - `context.attach(handle)` registra esse backend como o ativo;
     - `handle.init(config)` abre janela, renderer e áudio (via SDL2).
   - **`sprite.load_default_font()`** — carrega a fonte padrão, se existir.
   - **`if (@hasDecl(Game, "start")) game.start();`** → aqui roda o seu `start`.
   - **Loop**, repetido enquanto `neko.lifecycle.keeps_running()` for verdadeiro:
     - `input.beginFrame()` — drena todos os eventos pendentes da janela e
       atualiza o estado de teclas/mouse.
     - `while (input.poll_event()) |ev| game.event(ev);` — seu `event` recebe
       `key_down`, `quit`, mouse etc.
     - calcula `dt` (segundos desde o frame anterior) a partir de
       `time.ticks_ms()`, com clamp de `max_dt`.
     - `game.update(dt)` — sua física/movimento.
     - `debug.tick()`, `draw.clear(cor)`, `game.draw()` — seus desenhos.
     - `screen.present()` — mostra o frame na tela.
     - `input.endFrame()`.
   - Quando a janela fecha (`.quit`) ou alguém chama `request_stop()`, o loop
     acaba. O `defer screen.shutdown()` libera tudo.

## Como `neko.draw.rect` chega na tela

```zig
neko.draw.rect(rect, cor, true)
  → src/core/graphics/draw.zig        fn rect(...)
  → context.get()                     // ?Backend  (o backend registrado no init)
  → e.draw_rect(rect, cor, true)      // método da interface abstrata
  → vtable.draw_rect(self.ptr, ...)   // salto para o backend
  → src/backends/sdl2: SDL_SetRenderDrawColor + SDL_RenderFillRect
```

No PS2, esse mesmo `rect()` termina em `gskit.prim.sprite`. O seu jogo não sabe
( nem precisa saber ) a diferença.

## Os callbacks do struct `Game`

São **todos opcionais**: `neko.app` usa `@hasDecl(Game, "...")`, que decide em
tempo de compilação se aquele método existe. Se você não declarar `event`, ele
simplesmente não é chamado.

| Método | Quando roda |
|--------|-------------|
| `start(self)` | uma vez, depois da janela pronta |
| `event(self, ev)` | a cada evento (tecla, mouse, quit) |
| `update(self, dt)` | a cada frame; `dt` em segundos |
| `draw(self)` | a cada frame, depois do `clear` |
| `stop(self)` | uma vez, ao sair |

`self: *Game` é o ponteiro para a sua instância. É por isso que o `main` passa
`&game`: o runner precisa do **tipo** (`Game`, para a introspecção) e do
**ponteiro** (`&game`, para chamar os métodos na sua instância).

Um detalhe do Zig: se um método não usa `self`, você precisa descartá-lo
(`_ = self;`), senão o compilador reclama de parâmetro não usado.

## Por que `dt` importa

`update` roda uma vez por frame, mas frames têm durações diferentes. Multiplique
tudo que é velocidade por `dt`:

```zig
self.x += self.vx * dt; // vx em pixels por SEGUNDO
```

Sem `dt`, o jogo fica rápido em máquinas rápidas e lento em máquinas lentas.

## `event` x `update`

- `event` é **ponto no tempo**: a tecla foi pressionada *neste* frame, a janela
  foi fechada, o mouse mexeu. Não tem `dt`.
- `update` é **contínuo**: passou `dt` segundos, atualize o mundo.

Regra prática: coisas instantâneas (jump, menu, trocar de tela) em `event`;
coisas contínuas (andar, gravidade, animação) em `update`.

## Três jeitos de rodar

- **Estruturado** (o exemplo `app.zig`): você declara um struct com métodos.

  ```zig
  try neko.app.run(Game, &game, config);
  ```

- **Só uma função** (menos código, input por estado):

  ```zig
  try neko.run(init, .{ .title = "Jogo", .width = 640, .height = 360 }, update);

  fn update(dt: f32) void {
      if (neko.input.key(.right)) x += 160 * dt;
      neko.draw.rect(..., true);
  }
  ```

- **Janela explícita + `switch` de eventos** (você manda no loop):

  ```zig
  var window = try neko.window.create(init, .{ .title = "Jogo", .width = 640, .height = 360 });
  defer window.close();

  while (window.is_open()) {
      window.begin_frame();                 // drena eventos e calcula dt
      while (window.poll_event()) |ev| {
          switch (ev) {
              .quit => window.close(),
              .key_down => |key| if (key.key == .escape) window.close(),
              else => {},
          }
      }
      if (!window.is_open()) break;

      neko.draw.clear(neko.Color.hex(0x0c0c12));
      // ... desenhe ...
      window.present();
      window.end_frame();
  }
  ```

Os três usam o mesmo core e o mesmo backend. Escolha o que for mais
confortável: `neko.app` (métodos), `neko.run` (uma função), ou `neko.window`
(loop explícito com `switch`).

## Modelo Godot (code-first)

Além do loop, a engine tem o modelo runtime do Godot, mas **sem editor e sem
`.tscn`**: uma **cena** é uma função/struct que monta uma árvore, e um
**script** é um `struct` com métodos `ready`/`process`/`draw`/`input`.

```zig
const Player = struct {
    x: f32 = 300,
    pub fn process(self: *Player, dt: f32) void {
        if (neko.input.key(.right)) self.x += 180 * dt;
    }
    pub fn draw(self: *Player, at: neko.Point) void {
        neko.draw.rect(neko.Rect.init(@intFromFloat(self.x + @as(f32, @floatFromInt(at.x))), 160, 40, 40), neko.Color.hex(0x66ccff), true);
    }
};

var player = Player{};

fn makeWorld(alloc: std.mem.Allocator) !*neko.Scene {
    const root = try neko.scene.create(alloc, .{ .name = "World" });
    _ = try neko.scene.addScript(root, &player, .{ .name = "Player" });
    return root;
}

// ...
window.switch_to(try makeWorld(neko.allocator())); // troca e libera a cena anterior
```

- `window.switch_to(cena)` = `change_scene` do Godot (libera a anterior; pra
  voltar, monte de novo).
- `window.push_scene` / `pop_scene` = overlays (pause, opções).
- `neko.scene.get_node("Hud/Score")` = o `$Hud/Score`.
- `neko.scene.find("Player")` = busca por nome na árvore.

O mapa completo Godot → Neko está em [godot.md](godot.md).
