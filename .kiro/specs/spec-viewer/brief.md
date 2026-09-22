# Brief: spec-viewer

## Problem

archgenworks 의 개발자는 `.kiro/specs/<feature>/` 아래 requirements / biz-process /
design / tasks 와 `.kiro/steering/` 문서를 반복해서 읽는다. 지금은 `cat`/에디터로
파일을 하나씩 열어야 하고, 어느 스펙이 어느 단계까지 승인됐는지(`spec.json`)와
tasks.md 의 체크박스 진행률을 한눈에 볼 수단이 없다. 스펙이 6개를 넘어가면서
"지금 어디를 읽어야 하는가"를 찾는 비용이 문서를 읽는 비용보다 커졌다.

## Current State

- `.kiro/specs/*/spec.json` 에 `feature_name`(구 `name`), `phase`, `approvals.{requirements,
  bizProcess,design,tasks}.{generated,approved}` 가 있다. 두 스키마가 혼재한다
  (`sample-signup` 은 `name` 키, `tasks` 승인 항목 없음).
- `.kiro/steering/` 에 roadmap / ticket-workflow / value-chain 이 있고 항상 로드되는
  프로젝트 메모리다. `.kiro/reference/` 는 게으른 로드 대상이다.
- 진행 상태 조회는 `$kiro-spec-status` 스킬(에이전트 세션 안에서만 동작)뿐이다.
- 뷰어용 코드는 없다. 프로젝트에 소스 디렉터리 자체가 아직 없다.

## Desired Outcome

- 터미널에서 `.kiro/` 루트를 열면 왼쪽에 **스펙 트리**(피처별 phase·승인 상태 배지,
  requirements→biz-process→design→tasks 고정 순서), 그 아래 **steering 그룹**이 보이고,
  오른쪽에 선택한 마크다운이 폭에 맞게 래핑되어 렌더링된다.
- tasks.md 는 체크박스를 집계해 트리 노드에 진행률(예: `3/12`)을 표시한다.
- 헤딩 인덱스로 문서 내 점프(`[`/`]`)와 TOC 를 제공한다.
- 파일이 바뀌면(에이전트가 스펙을 갱신하면) 스크롤 위치를 유지한 채 다시 그린다.
- GFM 테이블은 컬럼 폭을 fair-share 로 맞춰 잘리지 않게, 코드 펜스는 syntect 로
  하이라이트, `mermaid` 펜스는 박스 처리된 소스 텍스트로 표시한다.
- 두 `spec.json` 스키마를 모두 읽고, 파싱 실패한 스펙은 트리에서 경고 배지로 표시하되
  뷰어 전체가 죽지 않는다.

## Approach

**C: ratatui 0.30 + tui-tree-widget 0.24 + pulldown-cmark 0.13 이벤트 위 커스텀 렌더러.**

- 스펙 인식 기능(승인 배지·태스크 진행률·헤딩 TOC)은 렌더러가 파서 이벤트를 직접
  소유해야 나온다. 기성 `tui-markdown` 은 완성된 `Text` 만 돌려줘 이 지점을 막고,
  테이블·코드블록이 폭에서 잘리며, "experimental PoC" 를 자칭한다.
- `termimad` 는 본문 품질이 좋지만 ratatui 위젯이 아니라 트리 패널을 손으로 그려야
  하고 syntect·태스크리스트·각주가 없다.
- 순서: `tui-markdown` 으로 하루짜리 프로토타입을 만들어 레이아웃·키바인딩을
  검증한 뒤 렌더 함수만 교체한다. 두 접근 모두 같은 파서·트리 위젯을 쓰므로 교체
  비용이 낮다.
- 파서는 pulldown-cmark 고정. comrak 은 AST 가 불필요하고 릴리스 주기가 잦아
  semver 흔들림이 크며, markdown-rs 는 2025-04 이후 릴리스가 없다.
- 실빌드 검증 완료(2026-09-10, rustc 1.97.1): 클린 빌드 15.9s, 109 크레이트,
  ratatui-core/widgets 분리 충돌 없음, C 의존성 0(onig 없음, fancy-regex 백엔드),
  GPL/MPL 없음, yanked 없음. syntect 경유 `bincode 1.x`/`yaml-rust` unmaintained
  경고 2건은 취약점 아님.

## Scope

- **In**:
  - 스펙 트리(피처 → 문서 4종 고정 순서), steering 그룹, `spec.json` 두 스키마 파싱
  - phase / 승인 배지, tasks.md 체크박스 진행률
  - 마크다운 렌더러: 폭 인식 래핑, 중첩 리스트 걸린 들여쓰기, 인용, GFM 테이블
    fair-share 컬럼, 태스크리스트, 취소선, YAML 프론트매터 숨김, syntect 코드 펜스,
    `mermaid` 펜스 박스 표시, CJK 폭(unicode-width)
  - 헤딩 인덱스 기반 점프와 TOC 팝업
  - notify 8 기반 라이브 리로드(스크롤 위치 유지)
  - `.kiro` 루트 자동 탐지(cwd 상향 탐색) 및 경로 인자
- **Out**:
  - 파일 편집, `spec.json` 승인 플래그 토글(주인은 kiro 스킬)
  - 웹/HTML 출력, 서버 모드
  - mermaid 그래픽 렌더링, 이미지 표시
  - `.kiro/reference/` 및 임의 디렉터리 범용 브라우징(후속 확장 가능하되 이번 범위 아님)
  - `agw` 바이너리 통합(별도 바이너리로 확정)

## Boundary Candidates

- **스펙 모델**: `.kiro` 디렉터리 → 피처/문서/steering 구조체. `spec.json` 두 스키마
  정규화, tasks.md 진행률 집계. 순수 함수, TUI 무관 → 단독 테스트 가능.
- **마크다운 렌더러**: `&str` + 폭 → ratatui `Text` + 헤딩 인덱스. 파서 이벤트만
  입력, 파일시스템 무관 → 골든 테스트로 고정.
- **TUI 셸**: 레이아웃(트리/본문/TOC), 키바인딩, 스크롤 상태, 리로드 처리.
- **파일 감시**: notify 이벤트를 디바운스해 셸에 "다시 읽어라" 신호만 전달.

## Out of Boundary

- 스펙 내용의 유효성 검사·승인 워크플로(kiro 스킬 소유)
- 에이전트 실행·관찰(`observability-surface` 소유)
- 프로젝트 워크스페이스 `Cargo.toml`(`multi-module-worktree` 소유) — 이 바이너리는
  그 워크스페이스에 들어가지 않는다

## Upstream / Downstream

- **Upstream**: `.kiro/specs/*/spec.json` 스키마와 문서 4종 파일명 규약(kiro 스킬군이
  정의). 스키마가 바뀌면 스펙 모델 계층만 갱신한다.
- **가치사슬**: `VC-DEV-SPEC-VIEW`(Main `VC-DEV-SPEC`, Mega `VC-DEV`) ↔ biz-process
  L1 `BP-SPEC-VIEW`. `value-chain.md` §5-2 참조.
- **Downstream**: 없음(당장). 후속 후보 — `.kiro/reference/` 브라우징, biz-process ↔
  value-chain 링크 따라가기, `agw` 감사 로그 마크다운 열람.

## Existing Spec Touchpoints

- **Extends**: 없음
- **Adjacent**: `observability-surface`(tmux 대시보드 — 사람이 보는 표면이지만 대상이
  에이전트 상태이므로 겹치지 않음), `multi-module-worktree`(워크스페이스 `Cargo.toml`
  주인 — 이 바이너리는 별도 크레이트라 손대지 않음)

## Constraints

- Rust. 별도 바이너리(`agw` 서브커맨드 아님). 로드맵 언어 제약(rust/golang/zig) 준수.
- 의존성 핀: ratatui 0.30.x, crossterm 0.29, tui-tree-widget 0.24, pulldown-cmark 0.13,
  syntect 5.3 `default-features=false, features=["default-fancy"]`(순수 Rust 빌드),
  notify 8.2 + notify-debouncer-full(9 는 아직 RC), unicode-width 0.2, serde_json.
- ratatui 0.30 은 `ratatui-core`/`ratatui-widgets` 로 분리됨 — 서드파티 위젯은
  `ratatui-core ^0.1` 의존이어야 한다. `f.area()` 사용(`f.size()` 아님).
- `mdcat`(MPL-2.0, 업스트림 아카이브) 코드 복사 금지. `tui-markdown`(MIT/Apache) 은
  렌더러 템플릿으로 참고 가능.
- 파일 읽기 전용. 어떤 경로로도 `.kiro/` 아래를 쓰지 않는다.
- 스펙 문서는 ko 로 작성(spec.json `language`).
