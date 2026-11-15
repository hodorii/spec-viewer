# 업무 프로세스 - spec-viewer-tasks-view

## 정의
스펙 작업을 진행/리뷰하는 개발자를 위해, 작업 목록 문서를 구조화된 목록으로 조회하고 그 안의 메타 태그로 참조 위치까지 바로 이동하게 하는 사용자 시각 프로세스이다.

## 가치사슬 매핑

| L1 Process | Unit Process (valueChainRef) | 가치 | 관련 요구사항 |
|------------|------------------------------|------|---------------|
| BP-TASKS-VIEW | VC-DEV-SPEC-VIEW | 터미널에서 스펙, 문서를 읽기 전용으로 빠르게 파악한다(작업 목록을 구조화해서 보여주고 근거로 바로 이동시켜 "빠르게 파악" 가치를 확장) | 1~6 |

## L1 Process: 작업 목록 구조화 조회와 참조 이동  (valueChainRef: VC-DEV-SPEC-VIEW)

개발자가 작업 목록 문서를 열었을 때, 각 작업의 상태, 난이도, 의존 관계, 근거를 구조화된 모습으로 확인하고, 그 근거로 바로 이동한다.

### L2 Activity: Tasks 문서를 구조화된 목록으로 본다  (1.1~1.5)
### L3 FunctionGroup/UI: Tasks 뷰 패널
### L4 Step: 작업 목록 문서를 선택한다
### L5 DetailStep: 방법론 설정이 지정한 작업 목록 문서인 경우
        Logic(AST):
          - IF 체크박스 줄이 하나 이상 있음 THEN 번호, 상태, (P) 표식과 함께 구조화된 목록으로 표시 (1.1)
          - ELSE 오류 없이 빈 목록 또는 안내 문구 표시 (1.5)
##### L4 Step: 전용 전환 키를 누른다
###### L5 DetailStep: 구조화 뷰 <-> 원문 렌더링 전환
        Logic(AST):
          - IF 현재 구조화 뷰 THEN 원문 마크다운 렌더링으로 전환 (1.2)
          - ELSE IF 현재 원문 렌더링이고 작업 목록 문서 THEN 구조화 뷰로 복귀 (1.3)
          - ELSE(작업 목록 문서가 아님) 아무 동작 없음 (1.4)

### L2 Activity: 작업 메타데이터 확인  (2.1~2.3)
#### L3 FunctionGroup/UI: Tasks 뷰 패널
##### L4 Step: 작업 줄 아래 태그를 본다
###### L5 DetailStep: 메타 태그 배지 표시
        Logic(AST):
          - `_Requirements:_`/`_Boundary:_`/`_Depends:_`/`_Difficulty:_`/`_BizProcess:_` 태그를 본문과 구분되는 배지, 서브라인으로 표시 (2.1)
          - `(P)` 표식을 병렬 가능 작업 표시로 함께 보여줌 (2.2)
          - `DONE`과 `_DoneWhen:_` 두 표기를 동일한 완료 기준으로 인식해 표시 (2.3)

### L2 Activity: 메타 태그 참조 위치로 이동  (3.1~3.4, 4.1~4.2)
#### L3 FunctionGroup/UI: Tasks 뷰 패널 + Navigator
##### L4 Step: 태그의 번호나 이름을 선택해 이동을 실행한다
###### L5 DetailStep: `_Requirements:_`/`_Boundary:_` 태그
        Logic(AST):
          - 방법론 설정의 매핑으로 대상 파일(requirements.md 또는 bugfix.md)을 찾는다
          - IF 대상 파일이 있고 그 번호가 원문에 있음 THEN 그 문서로 전환하고 해당 줄로 위치 이동, 트리 선택도 갱신 (3.1, 3.4)
          - ELSE 이동 실패를 알리고 현재 화면 유지 (4.1, 4.2)
###### L5 DetailStep: `_Depends:_` 태그
        Logic(AST):
          - IF 같은 작업 목록 문서 안에 그 작업 번호가 있음 THEN 문서 전환 없이 그 줄로 위치 이동 (3.2)
          - ELSE 이동 실패를 알리고 현재 화면 유지 (4.2)
###### L5 DetailStep: `_BizProcess:_` 태그
        Logic(AST):
          - 방법론 설정의 매핑으로 대상 파일(biz-process.md)을 찾는다
          - 태그 값은 고유 ID가 아니라 L1/L2 제목 텍스트 자체(예: "작업 메타데이터 확인") - 원문에서 그 제목 줄을 찾는다
          - IF 대상 파일이 있고 그 제목 문자열이 원문에 있음 THEN 그 문서로 전환하고 해당 줄로 위치 이동, 트리 선택도 갱신 (3.3, 3.4)
          - ELSE 이동 실패를 알리고 현재 화면 유지 (4.1, 4.2)

### L2 Activity: 방법론 구조 설정 반영  (5.1~5.4)
#### L3 FunctionGroup/UI: (화면 없음 - 설정 반영 결과로만 관찰)
##### L4 Step: 방법론 설정을 적용한다
###### L5 DetailStep: 설정값 존재 여부에 따른 분기
        Logic(AST):
          - 작업 목록 문서명, 태그->파일 매핑은 코드 고정값이 아니라 전부 방법론 설정값 (5.1)
          - IF 프로젝트 설정이 있음 THEN 그 설정(작업 목록 문서명, 태그-파일 매핑)을 따름 (5.3)
          - ELSE `.kiro` 기본값을 따름 - 작업 목록=tasks.md, `_Requirements:_`/`_Boundary:_`->requirements.md/bugfix.md, `_BizProcess:_`->biz-process.md, `_Depends:_`->자기 자신 (5.2, 5.4)

### L2 Activity: 기존 열람 경험 무변화  (6.1~6.5)
#### L3 FunctionGroup/UI: (화면 전체 - 결과로만 관찰)
##### L4 Step: 평소처럼 뷰어를 사용한다
###### L5 DetailStep: 불변 동작 확인
        Logic(AST):
          - 트리(좌), 뷰어(우) 2패널 레이아웃이 바뀌지 않음 (6.1)
          - 작업 목록 문서가 아닌 문서의 렌더링, 조작이 이전과 동일함 (6.2)
          - 트리 패널의 진행률 배지(n/total)가 기존과 동일하게 표시됨 (6.3)
          - 방법론 설정의 작업 목록 문서로 지정되지 않은 문서는 구조화 뷰, 메타 태그 표시가 나타나지 않고 기존 렌더링 그대로임 (6.4)
          - 구조화된 Tasks 뷰에서도 문서 내 검색이 오류 없이 동작함 (6.5)

### 검토 요청 (L1: 작업 목록 구조화 조회와 참조 이동)
승인 또는 수정 사항을 입력하세요.
