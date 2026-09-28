# Research & Design Decisions — spec-viewer-kiro-folder-groups

## Summary
- **Feature**: `spec-viewer-kiro-folder-groups`
- **Discovery Scope**: 신규 아키텍처 일반화(`design-discovery-full.md` 적용) — 핵심 데이터 모델(`SpecRoot`/`NodeId`)을 바꾸고 여러 파일에 파급되므로 "기존 패턴 재사용"만으로 끝나는 경량 확장이 아니다.
- **Key Findings**:
  - `src/app/loader.rs::load_snapshot`가 `root/specs`와 `root/steering` 두 경로만 하드코딩 스캔한다(`read_spec_dirs`/`read_steering_files`). 그 외 하위 폴더는 완전히 무시된다.
  - `src/spec/mod.rs::SpecRoot`는 `specs: Vec<Spec>`과 `steering: Vec<SteeringDoc>`(평평한 목록, 하위 폴더 미지원)만 가진다. `SteeringDoc.inclusion`은 `steering/*.md` 파일 내용을 미리 읽어 프론트매터를 파싱해둔 값이다.
  - `TreeSource::Files(FsTree)`(`--all` 모드)가 이미 재귀 트리를 완전히 구현해뒀다: `FsTree::scan(root)`(숨김파일/`.gitignore` 제외, 깊이 6 제한, 마크다운으로 이어지는 폴더만 유지, 경로순 정렬), `NodeId::Dir(PathBuf)`/`NodeId::File(PathBuf)`, `ui::tree_panel::build_files_items`(평탄한 `FsEntry` 목록 → 중첩 `TreeItem`), `app::search::flatten_files`(같은 목록 → 검색용 평탄화). 이 넷을 그대로 재사용하면 새 재귀 트리 순회/렌더링 로직이 전혀 필요 없다.
  - `app::mod::resolve_selection`/`current_editable_path`/`handle_tree_click`/`expand_all`은 이미 `NodeId::Dir`/`File`을 "폴더/파일"로 범용 처리하는 분기를 갖고 있다(`--all` 모드용) — `TreeSource::Kiro`가 이 두 variant를 만들어내는 소스가 되기만 하면 대부분의 통합(4.1, 4.3, 4.4)이 추가 코드 없이 성립한다. 유일하게 명시적으로 바꿔야 하는 곳은 `resolve_selection`의 `(TreeSource::Kiro(_), Some(NodeId::File(_))) => DocView::Empty`(현재는 "일어날 수 없는 조합"으로 처리됨) — 이제 실제로 일어나므로 `loader::load_doc`을 호출하도록 바꿔야 한다.
  - `steering`의 `inclusion` 배지는 파일 내용을 미리 읽어야만 계산 가능한데, 그 외 그룹(reference/guide 등)은 `--all` 모드와 동일하게 파일 내용을 선택 시점에만 읽는다(지금까지 관례). 배지 필요 여부에 따라 두 갈래의 로딩 전략이 생기는 것은 요구사항 3.1/3.2가 명시한 비대칭이므로 자연스럽다.

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| A. `NodeId::Dir`/`File` + `FsTree` 재사용, `SteeringDoc`/`NodeId::Steering`/`SteeringGroup` 폐기 (채택) | 자동 발견 그룹(steering 포함)을 전부 "이름 있는 `FsTree`"로 표현 | `--all` 모드가 이미 검증한 재귀 트리 순회/렌더링/검색 코드를 그대로 재사용(SSoT, Simplification) — 새 코드는 발견·조립·배지 부분뿐 | `NodeId::Steering`/`SteeringGroup`을 참조하는 기존 코드(약 6개 파일)를 모두 고쳐야 함 | 채택 — 재귀 지원이 필수 요구사항이 된 이상 `SteeringDoc`의 평평한 모델은 어차피 폐기해야 함 |
| B. `NodeId::Steering`류를 유지하고 별도로 `NodeId::Group`/`NodeId::GroupFile` 같은 새 variant를 추가 | steering과 새 그룹을 별개 타입 계열로 유지 | 기존 steering 코드 무변경 | steering도 재귀를 지원해야 하므로 결국 steering도 새 재귀 로직이 필요 — 사실상 두 벌의 "재귀 트리 + 배지" 구현이 생김(SSoT 위반) | 기각 |
| C. 모든 그룹(steering 포함)의 파일 내용을 미리 읽어 배지 인프라를 일반화 | 나중에 다른 그룹도 배지를 가질 가능성 대비 | 확장성 | 현재 요구사항에 없는 가정(YAGNI), reference/guide처럼 큰 폴더에서 불필요한 디스크 읽기 발생 | 기각(No speculative abstraction) |

## Design Decisions

### Decision: `SpecRoot.steering: Vec<SteeringDoc>`를 `SpecRoot.groups: Vec<KiroGroup>`로 대체
- **Context**: `steering`도 재귀 트리가 되어야 하므로 지금의 평평한 `SteeringDoc` 목록은 요구사항을 만족할 수 없다.
- **Selected Approach**: `KiroGroup { tree: FsTree, inclusion: Vec<(PathBuf, Inclusion)> }`. 그룹 이름은 별도 필드로 저장하지 않고 `tree.root`의 파일명에서 파생한다(SSoT — 중복 저장 금지).
- **Rationale**: `FsTree`를 그대로 재사용(Build vs Adopt). `inclusion`은 `steering`이라는 이름의 그룹에서만 채워지고 그 외는 빈 벡터 — 그룹 수가 적고(사람이 만드는 문서 폴더 수) 각 그룹의 파일 수도 적으므로 `Vec` 선형 탐색으로 충분(해시맵 도입은 과설계).
- **Trade-offs**: `NodeId::Steering`/`SteeringGroup`을 참조하던 기존 코드(로더/렌더러/검색/리듀서/테스트)를 전부 `Dir`/`File` 기반으로 다시 써야 한다.

### Decision: 그룹 자동 발견은 `.kiro` 바로 아래 폴더를 순회하며 `FsTree::scan`으로 판정
- **Context**: "마크다운이 하나라도 있으면(하위 폴더 포함) 자동 표시"를 판정해야 한다.
- **Selected Approach**: `.kiro` 아래 `specs`가 아닌 각 하위 디렉터리(닷파일 제외)에 대해 `FsTree::scan(dir)`을 호출하고, `entries`가 비어있지 않으면(=재귀적으로 마크다운을 하나라도 찾음) 그룹으로 채택한다.
- **Rationale**: `FsTree::scan` 자체가 이미 "마크다운으로 이어지는 디렉터리만 유지"를 구현해뒀으므로(Simplification), 별도의 "마크다운 존재 여부" 사전 검사 로직이 필요 없다 — 스캔 결과의 `entries.is_empty()`가 곧 그 판정이다.
- **Trade-offs**: 그룹 폴더 수만큼 `FsTree::scan`을 반복 호출 — 이 크레이트의 규모(`.kiro` 프로젝트 하나)에서는 비용이 무시할 만하다(기존 `spec::build`도 매 리프레시마다 전체를 다시 읽는 정책).

### Decision: 그룹 표시 순서는 이름 오름차순으로 정렬
- **Context**: 요구사항 1.5 "그룹들은 실행마다 일관된 순서로 표시됨" — `fs::read_dir`은 순서를 보장하지 않는다.
- **Selected Approach**: 발견된 그룹들을 이름(폴더명) 오름차순으로 정렬해 `SpecRoot.groups`에 담는다.
- **Rationale**: 가장 단순하고 결정적인 정렬 기준. 그룹 수가 적어 알파벳 순서 자체의 유용성보다 "항상 같은 순서"라는 안정성이 핵심.
- **Trade-offs**: 없음(간단한 고정 규칙).

### Decision: `ui::tree_panel::build_files_items`/`app::search::flatten_files`를 프리픽스·배지 조회 파라미터로 일반화
- **Context**: 그룹은 `Kiro` 트리 안에 여러 개가 나란히 들어가므로, `--all` 모드처럼 트리 전체가 하나의 `FsTree`가 아니라 "그룹 이름 노드 + 그 아래 `FsTree` 내용"이 여러 벌 붙는 구조다. 기존 `build_files_items`/`flatten_files`는 자신이 트리의 유일한 최상위라고 가정하고 있어 검색 하이라이트용 `own_path`/`NodeId` 경로에 그룹 접두사가 빠진다.
- **Selected Approach**: 두 함수 모두 `prefix: &[NodeId]`(경로 접두사) 인자를 추가한다. `build_files_items`는 추가로 `inclusion: &[(PathBuf, Inclusion)]`을 받아 파일 항목에 배지를 붙일지 결정한다. 기존 `--all` 모드 호출부는 `prefix: &[]`, `inclusion: &[]`로 호출해 동작이 완전히 그대로 유지된다.
- **Rationale**: Generalization 렌즈 — "인터페이스만 일반화하고 구현은 늘리지 않는다." 새 순회 로직을 만들지 않고 기존 함수의 계약만 넓힌다.
- **Trade-offs**: 두 함수의 시그니처 변경이 `--all` 모드 호출부와 그 테스트에도 인자를 하나(또는 둘) 추가해야 하는 나비효과를 만든다 — 범위가 명확하고 기계적인 변경이라 수용.

### Decision: `resolve_selection`의 `(Kiro, NodeId::File)` 분기를 "일어날 수 없는 조합"에서 실제 로딩으로 변경
- **Context**: 지금은 `TreeSource::Kiro`가 `NodeId::File`을 만들 일이 없어 `DocView::Empty`로 처리돼 있다. 그룹 도입 후에는 실제로 일어난다.
- **Selected Approach**: `(TreeSource::Kiro(_), Some(NodeId::File(path))) => loader::load_doc(path, width)`로 변경(`TreeSource::Files`의 동일 분기와 완전히 같은 처리).
- **Rationale**: 파일 로딩은 소스에 무관하게 "경로에서 마크다운을 읽어 렌더링"이라는 동일한 연산 — `loader::load_doc`을 그대로 재사용.
- **Trade-offs**: 없음.

## Risks & Mitigations
- `NodeId::Steering`/`SteeringGroup` 제거가 이 두 variant를 참조하는 기존 테스트(로더/렌더러/검색/리듀서/`tests/integration.rs`)를 전부 컴파일 에러로 깨뜨린다 — task 분해 시 "데이터 모델 변경과 그 downstream 반영을 한 태스크 그룹으로 순차 처리"하고, 매 태스크마다 `cargo test --no-run`(워크스페이스 전체)을 검증 명령에 필수로 포함한다.
- `steering` 폴더에 마크다운이 아예 없어지면(사용자가 다 지움) 그룹 자체가 안 보이게 된다 — 기존에도 `root/steering`이 없으면 빈 목록이었으므로 동작 변화 없음(요구사항 1.6과 일치).
- 배지 조회가 `steering`이라는 "이름"에 의존 — 사용자가 폴더명을 바꾸면 배지가 사라진다. 이는 요구사항 3.1/3.2가 명시적으로 "이름이 steering인 폴더"라고 못박은 대로이므로 의도된 동작이다.

## References
- 없음(신규 외부 라이브러리 도입 없음 — 기존 `ignore`/`tui-tree-widget` 재사용).
