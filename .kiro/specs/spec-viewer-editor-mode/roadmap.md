# Roadmap

## Overview
spec-viewer는 "파일 읽기 전용, `.kiro/` 아래를 쓰지 않는다"는 원칙 위에 서
있는 뷰어지만, 사용자는 뷰어 안에서 바로 vi/vim으로 전환해 지금 보고 있는
문서를 고치고 돌아올 수 있는 편집모드를 원한다. 리듀서(`app`)는 순수 상태
전이만 유지하고, 터미널 소유권을 쥔 `main`이 "일시정지 → 자식 프로세스 실행
→ 복귀" 시퀀스를 전담하는 구조로 접근한다. 편집 후 변경은 이미 존재하는
파일워처가 자동으로 감지해 재렌더하므로 별도 새로고침 로직은 필요 없다.

## Approach Decision
- **Chosen**: 리듀서-신호 / 메인-실행 분리 — `Action::Edit` → 새 `Control`
  변형(`Suspend(PathBuf)` 등)만 리듀서가 반환 → `main`이 raw mode·대체화면·
  마우스 캡처를 내렸다가 `$VISUAL`→`$EDITOR`→`vi` 순으로 자식 프로세스를
  실행·대기한 뒤 역순으로 복귀.
- **Why**: 기존 아키텍처(design.md Key Decision "Synchronous Core", `app`는
  순수 리듀서이고 터미널 I/O는 오직 `main`이 담당하는 계층 분리)를 그대로
  지키면서 최소 변경으로 편입 가능하다. 파일워처가 이미 재렌더를 처리하므로
  "편집 후 새로고침"을 위한 새 로직이 필요 없다.
- **Rejected alternatives**:
  - 뷰어 자체 인라인 에디터 구현 — "vi/vim 활용"이라는 요청 취지와 배치되고
    구현 비용이 훨씬 크다.
  - 리듀서 안에서 직접 `Command::spawn`+대기 — `app`가 `ui`/터미널을 모른다는
    기존 계층 분리(lib.rs 의존 방향: `spec`→`markdown`→`watch`→`app`→`ui`→
    `main`)를 깨뜨린다.

## Scope
- **In**: 문서 패널 파일 1개 편집 전환/복귀, 터미널 상태 완전 복원, 편집 후
  자동 재렌더(기존 워처), 에디터 선택 우선순위(`$VISUAL`→`$EDITOR`→`vi`),
  실패 시 에러 표시, 키맵/README 갱신.
- **Out**: `.kiro/` 승인 상태(`spec.json`) 조작, 자체 인라인 에디터, 동시
  다중 편집, `Definition`(합성 `## 정의` 뷰) 편집.

## Constraints
- Synchronous Core 원칙 유지(리듀서 비동기화 금지).
- `--no-watch`/마우스 캡처 실패(9.8 강등) 상태와의 상호작용은 requirements
  단계에서 명세를 확정한다(브리프의 미해결 질문 참고).

## Boundary Strategy
- **Why this split**: 편집모드는 `app`(신호)·`main`(실행)·`keymap`(키) 세
  컴포넌트를 가로지르는 하나의 응집된 기능이라 여러 스펙으로 쪼갤 이유가
  없다 — 단일 신규 스펙으로 진행한다(기존 bugfix 5개와 달리 신규 기능이라
  `kiro-spec-init`~`kiro-spec-design` 정규 경로를 그대로 밟는다).
- **Shared seams to watch**:
  - `Control` enum에 변형을 추가하면 `main.rs`의 모든 `Control` 매치 지점에
    파급된다 — 컴파일 타임에 강제되지만 놓친 분기가 없는지 확인 필요.
  - `Action` enum 확장이 `keymap::BINDINGS`(키→동작명 표, 도움말 자동 생성
    메커니즘)와 어긋나지 않는지.
  - 본편 `design.md`의 Out-of-Scope/Constraints 문구 개정이 다른 bugfix
    스펙들의 전제(“spec-viewer는 절대 쓰지 않는다”)와 충돌하지 않는지.

## Specs (dependency order)
- [ ] spec-viewer-editor-mode -- 문서 패널에서 `$VISUAL`/`$EDITOR`/`vi`로
  전환해 편집 후 복귀, 기존 파일워처로 자동 재렌더. Dependencies: none
