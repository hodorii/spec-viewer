# Design — spec-viewer-watch-startup-latency

## 정의
`watch::start`가 감시 루트 등록 시 파일 ID 캐시를 위해 전체 디렉터리를 선(先)순회·stat하는 비용을 없애, 대형 디렉터리(예: `node_modules`, `target`)를 여는 첫 화면 지연을 제거하는 최소 변경이다.

## 원인 (Root Cause)
- `spec-viewer/src/watch/mod.rs::start()` (17행)는 `notify_debouncer_full::new_debouncer(..)`를 호출한다. 이 함수는 `RecommendedCache`(`notify-debouncer-full` `cache.rs:121`)를 기본 캐시로 쓰는데, macOS(FSEvents 백엔드는 rename cookie를 주지 않음)에서는 `RecommendedCache = FileIdMap`이다(`cache.rs:118-121`, `#[cfg(any(target_os = "linux", target_os = "android"))]`가 아닌 분기).
- `debouncer.watch(root, RecursiveMode::Recursive)` → `add_root` (`notify-debouncer-full` `lib.rs:571-584`) → `FileIdMap::add_path` (`cache.rs:71-86`)가 `WalkDir::new(path).follow_links(true).max_depth(usize::MAX)`로 루트 아래 **모든 파일**을 재귀 순회하며 파일마다 `get_file_id`(stat)를 호출한다 — `.gitignore`/숨김/깊이 제한이 전혀 없다. `spec::FsTree::scan`이 같은 트리에 이미 적용하는 `ignore::WalkBuilder` 필터와 무관하게 별도로 전수 순회가 한 번 더 일어난다.
- `src/main.rs::main()`은 `watch::start(&root, tx)`를 `AppState::new`/`run_loop` 이전에 호출하므로(343-362행), 이 전수 순회가 끝나야 `run_loop`의 첫 `terminal.draw`가 실행된다 — 재현 절차에서 측정한 297ms~3.37s는 이 순회+stat 비용이다(재현 절차 참조; `FsTree::scan` 자체는 2~9ms로 무관함을 별도 측정으로 배제).
- `FileIdMap`이 존재하는 이유는 오직 "OS가 rename cookie를 안 주는 백엔드에서 삭제+생성 이벤트를 rename으로 재조합"하기 위함(`cache.rs:44-45` 문서 주석)인데, `watch::FsEvent { paths: Vec<PathBuf> }`(현재 코드)는 이벤트 종류(생성/삭제/rename)를 구분하지 않고 경로만 전달하므로 이 재조합 결과를 애초에 소비하지 않는다.

## 수정 방식
`new_debouncer(..)`(캐시 자동 선택) 대신 `new_debouncer_opt::<_, notify::RecommendedWatcher, notify_debouncer_full::NoCache>(..)`를 `NoCache::new()`와 함께 호출한다. `NoCache`는 `add_path`/`remove_path`가 아무 순회도 하지 않는 빈 구현(`cache.rs:106-114`)이므로 등록 비용이 감시 대상 파일 수와 무관해진다. `Watch::Live`의 캐시 타입 파라미터를 `RecommendedCache`에서 `notify_debouncer_full::NoCache`로 바꾼다. `RecursiveMode::Recursive` 등록 자체(요구사항 7.1/7.4/7.5가 기대하는 범위)는 그대로 유지한다 — OS 수준 재귀 감시(FSEvents)는 트리 크기와 무관하게 빠르며, 느린 것은 크레이트가 얹는 파일 ID 캐시 선순회뿐이었다.

**기각한 대안**: `FsTree::scan`이 계산한 gitignore 필터링된 디렉터리 목록만 개별 등록(비재귀)하는 방안 — 새로 생긴 하위 디렉터리를 추적하려면 복잡한 재등록 로직이 추가로 필요하고, `.kiro` 모드(디렉터리 목록 없이 루트 하나만 재귀 감시)에는 애초에 적용할 대상이 없어 두 모드에 서로 다른 수정이 필요해진다. `NoCache` 치환은 두 모드 모두에 동일하게 적용되는 한 줄 수준의 변경이라 최소성 원칙에 더 부합한다.

## 검증 속성
- (a) 결함 재현: 수정 전 `watch::start(<target/ 등 파일 수 10만+ 인 디렉터리>, ..)`가 수백 ms~수 초 걸림을 단위 테스트로 확인(1.1/1.2).
- (b) 기대 동작: 수정 후 동일 입력에서 `watch::start`가 디렉터리 안 파일 수와 무관하게 일정한(예: 100ms 미만) 시간 내 반환함을 같은 테스트로 확인(2.1/2.2).
- (c) 불변 동작: 기존 `watch::mod` 테스트(`test_start_live_event` 등, 파일 변경 → `FsEvent` 수신)가 `NoCache`로도 그대로 통과 — 2초 이내 변경 감지(3.1)가 캐시 종류에 좌우되지 않음을 확인. `--no-watch`(3.2)·`.kiro` 소형 루트 경로(3.3)는 이번 변경이 건드리지 않는 코드 경로이므로 기존 테스트로 회귀 여부만 재확인한다.

## 영향 범위
- 변경 파일: `spec-viewer/src/watch/mod.rs`만 (import 교체, `new_debouncer` → `new_debouncer_opt` 호출부, `Watch::Live`의 캐시 타입 파라미터).
- `spec-viewer` design.md의 Watch 관련 Boundary Commitment("`Watch::start(root) -> Watch::Live(Debouncer) | Watch::Manual(reason)`")는 함수 시그니처·반환 타입 형태를 그대로 유지하므로 위반 없음 — `Debouncer`의 제네릭 캐시 타입 파라미터만 바뀐다.
