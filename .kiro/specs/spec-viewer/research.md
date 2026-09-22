# Research & Design Decisions — spec-viewer

## Summary
- **Feature**: `spec-viewer`
- **Discovery Scope**: New Feature (그린필드, 코드 없음)
- **Key Findings**:
  - `Paragraph` 래핑에 스크롤을 맡기면 줄 수를 알 수 없고(`line_count` 는 unstable 피처) 매 프레임 재래핑 → 렌더러가 라인을 미리 만들고 앱이 인덱스로 스크롤
  - syntect 는 문법당 첫 강조에 16ms(release)/~150ms(debug) → 코드블록 단위 캐시 없이는 스크롤이 끊김
  - 폭 계산은 ratatui-core 가 쓰는 `UnicodeWidthStr::width` 와 같은 함수를 써야 렌더가 어긋나지 않음 (한글은 `width`/`width_cjk` 모두 2)
  - notify 는 디바운서 생성과 `watch` 양쪽에서 실패할 수 있음(inotify 한계 등) → 두 상태 `Live | Manual` 로 폴백

## Research Log

### 크레이트 선정 (discovery 단계, 2026-09-10)
- **Context**: 터미널 마크다운 뷰어의 파서·렌더 조합 결정
- **Sources Consulted**: crates.io, docs.rs, joshka/tui-markdown, Canop/termimad, leboiko/markdown-reader, charmbracelet/glow
- **Findings**: pulldown-cmark 0.13.4(MIT, 활발) / comrak 0.55(AST 불필요, 잦은 릴리스) / markdown-rs(2025-04 이후 정체). tui-markdown 은 완성 `Text` 만 반환해 헤딩 앵커·검색 매핑 불가, 테이블 잘림. termimad 는 ratatui 위젯이 아님
- **Implications**: pulldown-cmark 이벤트 위 커스텀 렌더러. 실빌드 검증: rustc 1.97.1, 109 크레이트, C 의존성 0, GPL/MPL 0, yanked 0

### ratatui 0.30 렌더 모델
- **Sources**: ratatui-core 0.1.2 소스(paragraph.rs, text.rs), 컴파일 프로브
- **Findings**: `Span/Line/Text` 는 `From<String>` 으로 `'static` 생성 가능. `Paragraph::scroll((y,x))` 는 Wrap 시 x 무시. `impl Widget for &Line` 존재 → 행 단위 직접 렌더 가능. `ratatui::init()` 이 panic hook + raw mode + alternate screen 처리
- **Implications**: `Rendered.lines: Vec<Line<'static>>` + 앱 스크롤 인덱스. panic 복원 코드 불필요

### tui-tree-widget 0.24
- **Findings**: `TreeItem<Identifier: Clone+Eq+Hash>`, 항목 텍스트는 `Text` (배지 span 가능), `TreeState::{select, open, key_up/down, flatten, selected()->&[Id]}`, 형제 ID 중복 시 `Err`
- **Implications**: `NodeId` 열거형이 조건 충족. 스펙 이름이 디렉터리명이라 형제 중복 없음. `flatten` 으로 가시 항목 탐색

### pulldown-cmark 0.13 이벤트 형상
- **Findings**: `TaskListMarker(bool)` 은 `Start(Item)` 직후; 헤더 셀은 `TableHead` 안의 `TableCell`(`TableRow` 없음); 프론트매터는 `MetadataBlock` Start/End 사이 `Text`; `Fenced(lang)` 로 mermaid 판별
- **Implications**: `block.rs` 상태기계가 위 순서를 전제. 검색은 렌더된 평문 위에서 하므로 `into_offset_iter` 불필요

### syntect 5.3 (default-fancy)
- **Findings**: 번들 테마 7종(`base16-ocean.dark` 가 어두운 배경에 안전), 문법 75종 — ts/toml/mermaid/dockerfile 없음. 로드 ~1ms, 문법당 첫 강조 16ms(release)
- **Implications**: `LazyLock`, 코드블록 `(해시, 언어)` 캐시, 미지원 언어는 평문 + 라벨. 테마는 어두운 배경 기본, `--theme` 옵션은 범위 밖

### 폭 계산과 래핑
- **Findings**: 이모지 ZWJ 시퀀스는 `char` 폭 합 ≠ `str` 폭. unicode-segmentation 은 ratatui-core 의존성이라 추가 비용 0
- **Implications**: grapheme 단위 누적, 문자열 폭은 `str::width()` 로만 측정

### notify 8.2 + debouncer-full 0.6
- **Findings**: `new_debouncer(timeout, None, handler) -> Result<_, notify::Error>`; `ErrorKind::{Io, MaxFilesWatch, PathNotFound, …}`; atomic save(rename) 는 `Modify(Name(Both))` 로 합쳐져 `paths=[from,to]`
- **Implications**: 생성·watch 어느 쪽 실패든 `Manual`; 경로가 루트 아래 `.md`/`spec.json` 이면 종류 무관 재로드

### 표 열 폭 배분
- **Sources**: comfy-table `arrange()` (dynamic.rs)
- **Findings**: 자연폭 ≤ 평균인 열 고정 → 평균 재계산 반복 → 남은 열 균등 → 셀 줄바꿈
- **Implications**: `table.rs` 가 같은 절차. 최소 열 폭 = 가장 긴 grapheme 단위 단어

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 순수 코어 + 단일 리듀서 (선택) | `spec`/`markdown` 순수, `app::update` 가 유일한 상태 전이 | 골든 테스트, 이벤트 재현 용이 | 리듀서 비대화 | Elm 스타일, ratatui 관례 |
| 위젯 중심 (각 패널이 자기 상태) | 패널이 자체 로드·갱신 | 코드 짧음 | 파일 이벤트가 패널마다 분산, 7.x 검증 어려움 | 기각 |
| 비동기 로더 스레드 | 렌더를 워커로 | 큰 문서 대응 | 요구사항에 크기 상한 없음, 복잡도 | 필요 시 후속 |

## Design Decisions

### Decision: 스크롤을 앱이 라인 인덱스로 소유
- **Alternatives**: `Paragraph::wrap + scroll` / 자체 라인 + 행 단위 렌더
- **Selected**: 후자
- **Rationale**: 검색 강조·헤딩 점프·리사이즈 위치 복원(5.13)·리로드 위치 유지(7.2)가 전부 "라인 인덱스" 한 값으로 표현됨
- **Trade-offs**: 래핑 코드를 직접 소유 (≈ wrap.rs 150행)
- **Follow-up**: 골든 테스트로 래핑 회귀 고정

### Decision: 일반화 — `DocView` 오류 상태를 하나의 열거형으로
- **Context**: 2.4 미생성, 3.6 파싱실패, 7.5 삭제, 8.3 권한없음, 8.4 비UTF-8 이 모두 "문서 패널에 뭔가 대신 보여줌"
- **Selected**: `DocView` 변형으로 통합, `doc_panel` 이 변형별 메시지만 분기
- **Rationale**: 오류 경로가 렌더 경로와 같은 타입을 흐르므로 앱이 죽을 틈이 없음

### Decision: mermaid 서브셋 자체 구현
- **Context**: 5.7/5.14 — 관계도·절차도·아키텍처(박스, 좌→우/위→아래)를 TUI에서 읽을 수 있어야 함. 요구 유형: flowchart·er·class·state(그래프형)·sequence.
- **Alternatives Considered**:
  1. 소스 박스만 표시 (기존 5.7) — 다이어그램이 핵심 정보인데 읽히지 않음
  2. 외부 mermaid 텍스트 렌더러 크레이트 채택 — 구현 착수 시 crates.io 확인 항목; 존재해도 C/JS 의존이면 배제(tech.md)
  3. 파서·레이아웃 자체 구현, 그래프형 4종은 공통 층 배치 + sequence — 선택
- **Selected Approach**: 3. 그래프형은 `Shape`/`EdgeStyle`만 다른 하나의 longest-path 층 배치 엔진 + 박스 문자, sequence는 열·생명선·행 단위 메시지. gantt/pie 등 비그래프 유형은 generic 파서로 행 단위 class 스타일 박스 그래프(5.15), 그것도 비면 소스 박스.
- **Rationale**: 요구 유형이 전부 노드-간선 그래프거나 sequence라 레이아웃 엔진은 2개면 되고, 교차 최소화 없이도 스펙 규모 다이어그램은 읽힌다.
- **Trade-offs**: 큰 그래프에서 간선 교차 발생 가능 → 폭 초과 규칙(5.16)으로 소스 폴백.
- **Follow-up**: 2.6/2.7 골든에 이 저장소 design.md 다이어그램 2개 고정.
- **Update (2026-09-17)**: sequence도 GraphEngine 레지스트리에 편입 — 지원 엔진(`dg`)이 있으면 그 박스+생명선 렌더링을 우선 쓰고, 실패·미지원 시 이 절의 전용 렌더러(`seq::layout`)로 폴백한다. 계기: `~/tools/dg`의 프레임 라벨 폭 버그를 고치면서 시퀀스 다이어그램 렌더링 품질이 재검토됨.

### Decision: 그래프 배선 — mdview 엔진 채택
- **Context**: 5.20 — 자체 층 배치는 간선을 공통 세로 버스에 `┼`로 합류시켜 출발·도착 추적이 불가(사용자 실물 확인).
- **Alternatives Considered**:
  1. 자체 구현에 띠 배선·교차 최소화 추가 — 배선 알고리즘을 처음부터 검증해야 함
  2. mdview `render/mermaid/graph.rs`(MIT, 동일 스택) 채택 + LR 전치 추가 — 선택
- **Selected Approach**: 2. 어댑터로 우리 파서 출력을 엔진 입력에 매핑, LR은 엔진의 층/띠를 전치.
- **Rationale**: 검증된 배선(barycenter·band·virtual node·feedback channel)을 재작성보다 싸게 얻고, 방향 유지(5.7)는 전치로 보존.
- **Trade-offs**: 외부 코드 3천행 유입·THIRD_PARTY 고지; 엔진 내부 변경은 상류 추적 필요.
- **Follow-up**: design.md Boundary Map(LR)·System Flows 실물 캡처로 검증.

### Decision: Build vs Adopt
- 채택: pulldown-cmark, syntect, tui-tree-widget, notify-debouncer-full, unicode-width
- 직접 구현: 래핑·표 배분·코드 박스 (tui-markdown 은 앵커/검색 매핑 부재로 기각), 검색 (문서 하나·평문 대상이라 라이브러리 불필요)

### Decision: 단순화
- 비동기 로더·워커 스레드 제거 (요구사항 없음)
- 렌더러 트레이트 추상화 없음 — 구현 하나
- `--theme`, 마우스, 북마크 제외

## Risks & Mitigations
- tui-tree-widget 단일 메인테이너 — 사용 API 가 작아 포크 비용 낮음
- syntect transitive `bincode 1.x`/`yaml-rust` unmaintained 경고 — 취약점 아님, `cargo audit` 경고로 추적
- 큰 문서에서 동기 렌더 지연 — 500행 < 50ms 회귀 테스트, 초과 시 워커 스레드 후속
- ratatui-core 0.x 변경 — 위젯 크레이트 3종 동시 갱신 필요, `Cargo.lock` 고정

## References
- https://docs.rs/ratatui/0.30.2 — Paragraph scroll/wrap 동작, init/restore
- https://docs.rs/tui-tree-widget/0.24.1 — TreeItem/TreeState API
- https://docs.rs/pulldown-cmark/0.13.4 — Event/Tag 형상, Options
- https://docs.rs/syntect/5.3.0 — HighlightLines, 번들 테마
- https://docs.rs/notify-debouncer-full/0.6.0 — new_debouncer, rename 병합
- https://github.com/Nukesor/comfy-table — 열 폭 배분 알고리즘
- https://github.com/joshka/tui-markdown — 렌더러 템플릿 참고 (MIT/Apache)
