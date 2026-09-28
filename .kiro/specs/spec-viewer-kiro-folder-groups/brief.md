# Brief: spec-viewer-kiro-folder-groups

## Problem
`.kiro` 프로젝트를 쓰는 사용자가 `specs/`/`steering/` 외에 `reference/`, `guide/` 같은 자기만의 하위 폴더를 두어도, spec-viewer는 그 존재를 전혀 보여주지 않는다. `.md` 파일이 있어도 트리에 나타나지 않아 탐색이 불가능하다.

## Current State
`src/app/loader.rs::load_snapshot`가 `root.join("specs")`와 `root.join("steering")` 두 경로만 하드코딩해서 스캔한다(`read_spec_dirs`/`read_steering_files`). `src/spec/mod.rs::SpecRoot`도 `specs: Vec<Spec>`과 `steering: Vec<SteeringDoc>` 필드만 가진다. `steering`은 폴더 바로 아래 평평한 `.md` 목록만 지원하고(하위 폴더 미지원), `inclusion` 프론트매터를 파싱해 배지로 보여준다. `specs`/`steering` 외의 어떤 하위 폴더도 완전히 무시된다.

## Desired Outcome
`.kiro` 아래(`specs` 제외) 어떤 하위 폴더든, 그 안에 `.md` 파일이 하나라도 있으면 자동으로 트리에 그룹으로 나타난다(폴더명을 코드에 하드코딩하지 않음). 각 그룹은 `--all` 모드의 `FsTree`처럼 하위 폴더까지 재귀적으로 펼쳐볼 수 있다. `steering` 폴더도 이 일반화된 그룹 중 하나가 되어 재귀 구조를 갖되, `inclusion` 배지 표시는 `steering`이라는 이름의 폴더에서만 유지된다.

## Approach
- **선택**: `SpecRoot`의 `steering: Vec<SteeringDoc>`(평평한 목록)을 없애고, `specs`를 제외한 모든 `.kiro` 하위 폴더를 `.md` 존재 여부로 자동 스캔해 `groups: Vec<KiroFolderGroup>` 같은 필드로 일반화한다. 각 그룹은 이름(폴더명)과 재귀적 트리(기존 `FsTree`/`NodeId::Dir`/`NodeId::File` 재사용)를 갖는다. `steering`이라는 이름의 그룹만 `inclusion` 배지 파싱을 추가로 적용한다.
- **이유**: `--all` 모드가 이미 재귀 트리(FsTree, Dir/File NodeId, 렌더링/검색/mouse-click 처리)를 구현해뒀으므로, 이를 재사용하면 새 트리 순회/렌더링 로직을 또 만들지 않아도 된다(Simplification, Build vs Adopt).

## Scope
- **In**: `.kiro` 하위(`specs` 제외) 폴더 자동 발견(마크다운 존재 시), 폴더별 재귀적 트리 표시, 기존 `steering`의 `inclusion` 배지를 `steering`이라는 이름의 그룹에 한해 유지, 문서 선택/편집/검색/watch와의 통합.
- **Out**: spec-kit(`.specify/`) 모드의 동등 기능(별도 스펙 대상 여부는 설계 단계에서 판단), `specs/` 자체의 표시 방식 변경, `TreeMode`/정렬 키/이번 세션에서 다룬 모드 전환·전체펼치기 키 변경.

## Boundary Candidates
- `src/spec/mod.rs` — `SpecRoot`/`NodeId`/`build()` 데이터 모델
- `src/app/loader.rs` — `load_snapshot`의 디렉터리 스캔 로직
- `src/ui/tree_panel.rs` — 그룹 렌더링(현재 `steering_group_item`/`steering_item`을 대체·일반화)
- `src/app/search.rs` — `flatten_tree`가 새 그룹 구조를 순회하도록 확장
- `src/app/mod.rs` — 문서 선택/편집 경로(`resolve_selection`, `current_editable_path` 등)가 새 NodeId를 처리하도록 확장

## Out of Boundary
- spec-kit 모드 자체의 폴더 발견 로직(`spec_kit.rs`)은 이번 스펙에서 건드리지 않는다(요청 범위가 `.kiro` 모드에 한정됨을 확인 필요 — 설계 단계에서 재확인).

## Upstream / Downstream
- **Upstream**: 기존 `.kiro`/`SpecRoot`/`SteeringDoc` 데이터 모델(원 `spec-viewer` 스펙), `--all` 모드의 `FsTree`/`NodeId::Dir`/`File`(재사용 대상).
- **Downstream**: 트리 렌더링(`tree_panel`), 검색(`search`), 문서 로더(`loader`), mouse 클릭 처리, 도움말/상태표시줄에 영향 없음(그룹은 기존 트리 안에 자연스럽게 편입).

## Existing Spec Touchpoints
- **Extends**: `spec-viewer`(원본, `SteeringDoc`/`SpecRoot` 정의), `spec-viewer-spec-kit-support`(대조 대상 — spec-kit은 이번 스펙 범위 밖).
- **Adjacent**: `spec-viewer-tree-navigation-modes`(전체펼치기/접기, 모드 전환 — 새 그룹 노드도 이 액션들과 호환돼야 함).

## Constraints
- 기존 `steering`의 `inclusion` 배지 표시를 깨뜨리지 않아야 한다(회귀 금지).
- `NodeId`는 `Eq`/`Hash`/`Clone` 등 `TreeState<NodeId>`가 요구하는 트레잇을 유지해야 한다.
- 새 그룹 스캔이 `specs/`를 다시 스캔하거나 이중 표시를 만들면 안 된다.
