# Design — spec-viewer-files-tree-scroll-latency

## 정의
`--all` 모드의 `Files` 트리 렌더가 매 프레임 전체 항목을 새로 할당하는 비용을 없애, 항목 수가 많은 디렉터리에서도 스크롤이 항목 수와 무관하게 반응하도록 캐시를 추가하는 최소 변경이다.

## 원인 (Root Cause)
- `spec-viewer/src/ui/tree_panel.rs::render_files` (96행)는 매 렌더 호출마다 `build_files_items(&tree.entries, search_matches)`를 실행한다(103행). 이 함수(122행)는 `tree.entries`(`FsTree`의 평평한 `Vec<FsEntry>`) 전체를 한 번 순회하며 항목마다 새 `PathBuf` 클론, `file_name_label`의 새 `String`, `Line`/`Span`, `TreeItem`을 할당한다 — **스크롤이 뷰포트 안에서 위치만 바꾸는 동작인데도**, 화면에 실제로 보이는 행 수가 아니라 트리 전체 항목 수에 비례하는 작업을 매 프레임 반복한다.
- `main.rs::run_loop`는 이벤트 1개당 `step()`(리듀서 1회 + `terminal.draw()` 1회)을 동기 실행하고 이벤트를 배치하지 않으므로(재현 절차 참조), 빠른 스크롤이 큐에 쌓은 이벤트 수만큼 `render_files`가 반복 호출된다.
- 격리 측정(재현 절차): 20,000개 항목에서 `build_files_items` 4.25ms/프레임, `frame.render_stateful_widget`(→ `tui_tree_widget`의 자체 `flatten()`) 1.8ms/프레임. `build_files_items`가 우리 코드가 직접 만든, 그리고 직접 없앨 수 있는 비용의 절반 이상을 차지한다. `render_stateful_widget`의 내부 `flatten()` 비용은 서드파티 크레이트(`tui_tree_widget`) 동작이라 이번 수정 범위 밖이다(Boundary Context).
- `render_kiro`(같은 파일, 65행)의 기존 주석("이 크레이트 규모에서는 캐시 없이도 저렴")은 `.kiro` 모드가 보통 수십 개 이하 스펙만 다루는 데서 나온 가정이며, `--all` 모드는 임의 디렉터리를 대상으로 하므로 이 가정이 깨진다.

## 수정 방식
`build_files_items`의 결과를 캐시해, `tree.entries`와 `search_matches`가 이전 렌더와 동일하면 재사용한다. `FsEntry`/`FsTree`는 이미 `#[derive(PartialEq, Eq)]`이므로(할당 없이) 동등성 비교로 변경 여부를 판단할 수 있다. 캐시는 `AppState`에 필드로 둔다 — 이미 `AppState.tree: TreeState<NodeId>`가 `tui_tree_widget`의 위젯 상태를 그대로 들고 있으므로(기존 설계 그대로), 같은 크레이트의 `TreeItem` 캐시를 추가하는 것은 새로운 계층 위반이 아니라 기존 패턴의 연장이다:

```rust
pub files_tree_cache: Option<(Vec<FsEntry>, Vec<Vec<NodeId>>, Vec<TreeItem<'static, NodeId>>)>,
```

`render_files`는 `state`(또는 캐시 슬롯에 대한 `&mut`)를 받아, `(tree.entries, search_matches)`가 캐시와 같으면 캐시된 `Vec<TreeItem>`을 그대로 쓰고, 다르면 재빌드 후 캐시를 갱신한다. `ui::render`가 이미 `&mut AppState`를 받으므로 시그니처 변경은 `tree_panel::render`/`render_files` 호출부에 캐시 슬롯을 추가로 넘기는 정도로 국한된다.

**기각한 대안**:
- `TreeSource::Files(FsTree)` → `TreeSource::Files(Rc<FsTree>)`로 바꿔 `Rc::ptr_eq`로 O(1) 무효화 판단: 더 빠르지만 `TreeSource`를 참조하는 모든 패턴 매치·테스트 픽스처를 건드려야 해 최소성 원칙에 맞지 않음. `Vec<FsEntry>` 동등성 비교(할당 없음, `String`/`Span`/`TreeItem` 생성 없음)만으로도 재구성 비용의 지배적인 부분(할당)을 제거하기에 충분하다.
- 모듈 전역 `thread_local!` 캐시: `AppState`를 건드리지 않지만, 테스트에서 상태가 감춰진 전역으로 새는 것이 이 크레이트의 명시적 상태-전달 스타일과 어긋나 채택하지 않음.
- `tui_tree_widget`의 `flatten()` 자체를 캐시/우회: 서드파티 크레이트 내부이며 이번 결함의 지배적 비용이 아니므로 범위 밖으로 둠(Boundary Context).

## 검증 속성
- (a) 결함 재현: 수정 전 20,000개 항목 `Files` 트리에서 60회 연속 스크롤 스텝(리듀서+렌더)이 임계값(예: 300ms) 이상 걸림을 단위 테스트로 확인(1.1/1.2).
- (b) 기대 동작: 수정 후 동일 입력에서 `entries`/`search_matches`가 변하지 않는 연속 스크롤은 임계값 이내로 끝남(캐시 재사용) — 첫 프레임과 두 번째 이후 프레임의 시간 차이로 캐시 적중을 확인(2.1). 항목이 바뀐 뒤(파일 추가/삭제 시뮬레이션 또는 검색어 변경) 다음 프레임에 새 내용이 반영됨을 별도로 확인(2.2).
- (c) 불변 동작: 기존 `tree_panel`/`app` 테스트(트리 검색 하이라이트, 펼치기/접기, `.kiro` 모드 렌더, `resync`의 `Files` 재스캔)가 캐시 도입 후에도 그대로 통과 — 3.1~3.4가 캐시 종류에 좌우되지 않음을 확인.

## 영향 범위
- 변경 파일: `spec-viewer/src/app/mod.rs`(`AppState`에 캐시 필드 추가 및 초기화), `spec-viewer/src/ui/tree_panel.rs`(`render_files`/`build_files_items` 호출부에 캐시 조회·갱신 추가), `spec-viewer/src/ui/mod.rs`(캐시 슬롯을 `tree_panel::render` 호출부로 전달, 필요 시).
- `spec-viewer` design.md의 Boundary Commitment("`ui`는 상태를 읽기만 한다")는 `AppState.tree: TreeState<NodeId>`가 이미 위젯이 직접 갱신하는 필드를 갖고 있어 이번 캐시 필드 추가도 같은 성격 — 업무 상태(`specs`, `docs` 등)는 건드리지 않으므로 위반 없음.
