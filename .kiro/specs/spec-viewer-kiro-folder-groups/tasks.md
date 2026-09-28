# Implementation Plan — spec-viewer-kiro-folder-groups

## 정의
spec-viewer 구현자를 위해 design.md 컴포넌트 경계(`spec`/`app::loader`/`ui::tree_panel`/`app::search`/`app::mod`)를 따라 관찰 가능한 DONE 상태 단위로 분해한 구현 계획이다.

- [ ] 1. spec 모듈: 그룹 데이터 모델과 조립 로직
  - DONE: `SpecRoot.steering: Vec<SteeringDoc>`가 `SpecRoot.groups: Vec<KiroGroup>`(각 `KiroGroup`은 재귀적인 `FsTree`와, 그룹 이름이 정확히 "steering"일 때만 채워지는 `inclusion` 목록을 가짐)로 대체된 것, `DirSnapshot.steering`이 `DirSnapshot.groups: Vec<GroupSnapshot>`로 대체된 것, `build()`가 각 `GroupSnapshot`을 `KiroGroup`으로 순서 보존 변환하며 `steering_contents`를 `inclusion()`으로 파싱해 채우는 것을 단위 테스트로 확인. 하위 폴더를 포함한 `FsTree`를 그대로 담으므로 몇 단계든 하위 폴더가 표현되는 것(재귀), `steering`이라는 이름의 그룹도 다른 그룹과 동일한 구조를 쓰는 것(특례는 `inclusion` 필드뿐)을 확인.
  - _Requirements: 2.1, 2.2, 2.3, 3.1, 3.2_
  - _Boundary: spec (SpecRoot, KiroGroup, DirSnapshot, GroupSnapshot, build)_
  - _Difficulty: mid_
  - _BizProcess: BP-KIRO-GROUPS.L2-C2_

- [ ] 2. `.kiro` 하위 폴더 자동 발견과 탐색 표시
- [ ] 2.1 (P) app::loader: 폴더 자동 발견과 재귀 스캔
  - DONE: `load_snapshot`이 `.kiro` 직계 하위 폴더 중 `specs`와 닷파일을 제외한 각 폴더에 재귀 스캔을 수행해, 마크다운을 하나도(하위 폴더 포함) 포함하지 않은 폴더는 버리고 포함한 폴더만 그룹 후보로 남기는 것, 그룹 후보가 여럿일 때 폴더명 오름차순으로 정렬되는 것, `specs`가 아니거나 마크다운이 없는 폴더가 그룹으로 나타나지 않는 것, `.kiro` 아래 그룹 후보가 하나도 없어도 오류 없이 빈 목록을 반환하는 것을 단위 테스트로 확인. 폴더명이 정확히 "steering"인 그룹에 한해 그 하위 모든 마크다운 파일의 내용을 읽어 `GroupSnapshot.steering_contents`에 채우고, 그 외 그룹은 항상 빈 목록인 것을 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 3.1, 3.2_
  - _Boundary: app::loader (load_snapshot)_
  - _Depends: 1_
  - _Difficulty: mid_
  - _BizProcess: BP-KIRO-GROUPS.L2-C1_
- [ ] 2.2 (P) ui::tree_panel: 재귀 트리 렌더링 일반화와 그룹 노드 표시
  - DONE: 기존 `--all` 모드 전용이던 트리 항목 조립 함수가 "이 트리가 더 큰 트리 안에 얼마나 깊이 중첩됐는지(조상 경로)"와 "파일별로 붙일 배지(있으면)"를 받도록 일반화되고, `--all` 모드 호출부는 조상 경로 없음·배지 없음으로 호출해 실제 렌더된 프레임이 기존과 동일한 것을 실물 렌더로 확인(무회귀). 각 그룹이 폴더명을 단 노드로 스펙 노드들 뒤에 나란히 나타나고, 그 아래 하위 폴더·파일이 몇 단계든 펼쳐볼 수 있는 것, 개별 노드의 펼침·접힘은 여전히 직계 한 단계에만 적용되는 것, "steering"이라는 이름의 그룹 파일에는 기존과 동일한 포함 방식 배지가 붙고 다른 그룹 파일에는 배지가 없는 것을 실제 렌더된 프레임으로 확인.
  - _Requirements: 1.1, 1.3, 2.1, 2.2, 2.3, 2.4, 3.1, 3.2_
  - _Boundary: ui::tree_panel (build_files_items, render_kiro, 그룹 노드 렌더링)_
  - _Depends: 1_
  - _Difficulty: high_
  - _BizProcess: BP-KIRO-GROUPS.L2-C2_
- [ ] 2.3 (P) app::search: 그룹 검색 평탄화
  - DONE: 트리 검색(이름 검색)의 순회 대상 목록에 각 그룹의 노드와 그 하위 폴더·파일까지 포함되는 것, `--all` 모드의 기존 검색 순회 결과가 그대로인 것(무회귀)을 단위 테스트로 확인.
  - _Requirements: 4.2_
  - _Boundary: app::search (flatten_tree, flatten_files)_
  - _Depends: 1_
  - _Difficulty: mid_
- [ ] 2.4 (P) app::mod: 그룹 파일의 선택·펼침·감시 통합
  - DONE: 그룹 안의 문서 파일을 선택하면 문서 패널에 렌더링된 내용이 표시되는 것(`--all` 모드의 동일 파일 선택과 같은 경로로), 그룹 노드나 그 하위 폴더가 마우스 클릭이나 전체펼치기 키로 폴더 노드와 동일하게 펼쳐지고 접히는 것을 단위/통합 테스트로 확인. "steering 전용 특수 처리" 관련 코드(옛 `NodeId::Steering`/`SteeringGroup` 분기)가 제거되고 그룹은 일반 폴더/파일 경로로만 처리되는 것을 확인.
  - _Requirements: 2.4, 4.1, 4.3, 4.4, 4.5_
  - _Boundary: app::mod (resolve_selection, expand_all, handle_tree_click)_
  - _Depends: 1_
  - _Difficulty: mid_
  - _BizProcess: BP-KIRO-GROUPS.L2-C4_

- [ ] 3. 통합과 전체 회귀 확인
- [ ] 3.1 전체 컴파일과 기존 기능 무회귀 확인
  - DONE: 워크스페이스 전체가 경고 없이 컴파일되고(`cargo test --no-run` 포함), 기존 스펙 트리·`--all` 모드·GitHub spec-kit 모드 관련 테스트가 모두 그대로 통과하는 것을 확인. spec-kit 모드 코드(`render_spec_kit`/`flatten_spec_kit`/`spec_kit::build`)가 이번 스펙으로 전혀 수정되지 않았음을 diff로 확인.
  - _Requirements: 1.4, 5.1_
  - _Depends: 2.1, 2.2, 2.3, 2.4_
  - _Difficulty: mid_
  - _BizProcess: BP-KIRO-GROUPS.L2-C5_
- [ ] 3.2 실물 렌더 E2E 확인
  - DONE: 실제 임시 `.kiro` 트리(specs 하나, `steering` 폴더에 하위 폴더 포함 문서, 가상의 다른 이름 폴더에도 하위 폴더 포함 문서)를 만들어 실제 렌더된 프레임에서 두 그룹이 모두 보이고, 각각 하위 폴더까지 펼쳐 문서를 확인할 수 있고, `steering` 문서에만 포함 방식 배지가 보이고 다른 그룹 문서에는 배지가 없는 것, 그룹 안 문서 이름/내용 검색이 스펙 문서와 동일하게 동작하는 것, 전체펼치기 키가 그룹과 그 하위 폴더까지 함께 펼치는 것을 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.5, 1.6, 2.1, 2.2, 2.3, 2.4, 3.1, 3.2, 4.1, 4.2, 4.4_
  - _Depends: 3.1_
  - _Difficulty: high_
  - _BizProcess: BP-KIRO-GROUPS.L1_

- [ ] 4. 검증
- [ ] 4.1 편집·감시 통합 실물 확인
  - DONE: 그룹 안 문서가 문서 패널에 표시된 상태에서 편집 키로 외부 에디터가 열리고 저장 내용이 반영되는 것, 파일 감시가 켜진 상태에서 그룹 폴더 아래 파일을 실제로 추가·수정·삭제했을 때 트리와 문서 패널이 자동으로 갱신되는 것을 실물 실행으로 확인.
  - _Requirements: 4.3, 4.5_
  - _Depends: 3.2_
  - _Difficulty: mid_
- [ ] 4.2 실제 컴파일된 바이너리 pty 스모크 테스트
  - DONE: 실제 컴파일된 릴리스 바이너리를 pty로 구동해, 임시 `.kiro` 트리에서 `steering` 외의 자동 발견 그룹이 실제 화면에 나타나고, 하위 폴더를 펼쳐 문서를 선택하면 실제 내용이 표시되고, `steering` 문서에만 배지가 보이는 것을 재확인.
  - _Requirements: 1.1, 2.1, 2.3, 3.1_
  - _Depends: 4.1_
  - _Difficulty: low_
