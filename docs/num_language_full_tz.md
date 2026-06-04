# Техническое задание: язык программирования Num

## 1. Название проекта

**Num** — язык программирования для надёжных AI-автоматизаций, backend-сервисов, бизнес-процессов, workflow-систем и программ, которые работают с чувствительными данными, внешними API, правами доступа, деньгами, документами и действиями в реальном мире.

Название происходит от идеи причины, ответственности и последствий: программа должна не только выполнять код, но и понимать, **почему действие разрешено, откуда взялись данные, насколько им можно доверять, сколько стоит операция и как отменить последствия ошибки**.

---

## 2. Краткое описание

Num — это статически типизированный язык программирования с встроенной моделью:

- происхождения данных;
- доверия к данным;
- приватности;
- прав доступа;
- аудита;
- транзакций и компенсирующих действий;
- работы с AI-моделями;
- неопределённости;
- лимитов стоимости и времени;
- безопасного взаимодействия с внешними сервисами.

Язык предназначен для систем, где ошибка программы может привести не просто к багу, а к реальным последствиям: утечке персональных данных, неверному финансовому действию, ошибочному юридическому выводу, неправильному ответу AI-агента, некорректной автоматизации или неоткатываемому бизнес-процессу.

---

## 3. Проблема, которую решает язык

Большинство современных языков хорошо решают задачи вычислений, но плохо выражают ответственность программы перед реальным миром.

Например, в Python, TypeScript, Go, Java или Rust можно написать backend-сервис, AI-агента или workflow-систему, но следующие аспекты обычно реализуются вручную:

- проверка прав доступа;
- защита персональных данных;
- отслеживание происхождения данных;
- аудит действий;
- rollback внешних операций;
- лимиты на стоимость API-вызовов;
- работа с неопределёнными AI-ответами;
- контроль доверенности данных;
- предотвращение случайной отправки приватных данных во внешний сервис;
- защита от prompt injection;
- проверка того, можно ли использовать конкретные данные в конкретном контексте.

В результате безопасность и надёжность зависят не от языка, а от дисциплины команды.

Num должен перенести эти гарантии на уровень языка, компилятора и runtime.

---

## 4. Цель проекта

Создать полноценный промышленный язык программирования, который позволяет разрабатывать безопасные и проверяемые системы для AI, бизнеса, backend-инфраструктуры и автоматизации.

Главная цель:

> Сделать язык, в котором программа описывает не только “что выполнить”, но и “можно ли это выполнять, откуда взялись данные, насколько им можно доверять, кто имеет право выполнить действие, сколько оно стоит и как его отменить при ошибке”.

---

## 5. Основная идея языка

Num должен объединять в себе сильные стороны нескольких подходов:

- строгая типизация как в Rust, Swift, Kotlin, TypeScript;
- безопасная работа с ошибками как в Rust и Go;
- удобство backend-разработки как в Go и TypeScript;
- выразительность бизнес-логики как в DSL/workflow-языках;
- встроенные ограничения безопасности как в policy engines;
- работу с AI как первоклассной частью языка;
- аудит и трассировку как системную возможность.

Главное отличие Num от обычных языков:

Обычные языки считают, что переменная — это просто значение.

Num считает, что значение имеет:

- тип;
- источник;
- уровень доверия;
- уровень приватности;
- разрешённые операции;
- срок жизни;
- контекст использования;
- возможную неопределённость;
- аудит-след.

---

## 6. Целевая аудитория

Язык предназначен для:

- backend-разработчиков;
- AI-инженеров;
- fintech-команд;
- legaltech-команд;
- healthtech-команд;
- enterprise-разработчиков;
- команд, строящих AI-агентов;
- разработчиков workflow-систем;
- разработчиков государственных и банковских сервисов;
- разработчиков CRM/ERP-систем;
- команд, которым важны безопасность, аудит и контроль доступа.

---

## 7. Основные сценарии использования

### 7.1 AI-агенты

Num должен позволять писать AI-агентов, которые:

- получают данные из разных источников;
- классифицируют запросы;
- принимают решения;
- вызывают внешние API;
- создают документы;
- отправляют сообщения;
- работают с правами пользователей;
- логируют действия;
- не могут случайно выполнить опасное действие без проверки.

Пример:

```num
workflow handle_ticket(ticket: Ticket from UserInput private) {
    let intent: Uncertain<Intent> = ai.classify(ticket.message)

    if intent.confidence < 0.85 {
        assign_to_human(ticket)
        return
    }

    match intent.value {
        RefundRequest => {
            require Permission.IssueRefund for current_user

            transaction {
                issue_refund(ticket.customer_id)
                notify_customer(ticket.email)
                audit("refund issued")
            }
        }

        BillingQuestion => {
            require Permission.ViewBilling for current_user

            let answer = ai.draft_reply(ticket.message)
            send_reply(ticket.email, answer)
        }
    }
}
```

---

### 7.2 Backend-сервисы

Num должен подходить для создания API, микросервисов, монолитов, внутренних сервисов и интеграционных систем.

Пример:

```num
service BillingApi {
    route POST "/refunds" requires Permission.IssueRefund {
        input request: RefundRequest from HttpBody private

        transaction {
            let payment = payments.find(request.payment_id)
            payment.refund()
            audit("refund_created", payment.id)
        }

        return Ok()
    }
}
```

---

### 7.3 Финансовые операции

Язык должен помогать избегать ошибок в денежных операциях:

- запрет смешивания валют;
- точные decimal-числа вместо floating point;
- обязательный audit trail;
- обязательная обработка ошибок;
- компенсирующие операции;
- лимиты суммы;
- подтверждение опасных действий.

Пример:

```num
fn transfer(
    from: Account,
    to: Account,
    amount: Money<KZT>
) requires Permission.TransferMoney {
    require amount <= current_user.transfer_limit

    transaction {
        from.debit(amount)
        to.credit(amount)
        audit("transfer_completed", amount)
    }
}
```

---

### 7.4 Работа с документами

Num должен поддерживать безопасную обработку документов:

- PDF;
- DOCX;
- изображения;
- таблицы;
- email-вложения;
- сканы;
- OCR;
- AI-анализ;
- проверку достоверности;
- юридические и финансовые документы.

Пример:

```num
let contract: Document from Upload private = files.read(upload_id)

let extracted: Uncertain<ContractData> = ai.extract(contract)

if extracted.confidence < 0.9 {
    require_human_review(extracted)
}
```

---

### 7.5 Корпоративные workflow

Num должен быть удобен для описания длинных бизнес-процессов:

- согласование договора;
- возврат денег;
- обработка заявки;
- onboarding клиента;
- проверка документов;
- создание счёта;
- уведомления;
- интеграции с CRM;
- работа с несколькими сервисами.

Пример:

```num
workflow approve_contract(contract: Contract) {
    legal.review(contract)
    finance.check_budget(contract)
    ceo.approve(contract)

    transaction {
        crm.mark_approved(contract.id)
        send_notification(contract.owner)
        audit("contract_approved")
    }
}
```

---

## 8. Главные принципы языка

### 8.1 Безопасность по умолчанию

Опасные действия должны быть запрещены, пока разработчик явно не докажет, что они разрешены.

Пример:

```num
let email: Email from UserInput private

external.crm.send(email)
```

Компилятор должен выдать ошибку:

```text
Cannot send private UserInput data to external service without explicit policy.
```

---

### 8.2 Явность вместо скрытой магии

Num не должен скрывать опасные операции.

Разработчик должен явно указывать:

- где внешнее действие;
- где AI-вызов;
- где приватные данные;
- где операция требует разрешения;
- где возможен rollback;
- где есть неопределённость;
- где есть стоимость;
- где нужен audit.

---

### 8.3 Компилятор как защитник

Компилятор должен не просто проверять синтаксис, а предотвращать архитектурные ошибки:

- утечки данных;
- отсутствие проверки прав;
- игнорирование неопределённости;
- отсутствие rollback для критического действия;
- смешивание доверенных и недоверенных данных;
- небезопасное логирование;
- превышение лимитов стоимости;
- использование AI-ответа как факта без проверки.

---

### 8.4 Полная трассируемость

Любое важное действие должно иметь audit trail:

- кто выполнил;
- когда;
- на каком основании;
- какие данные использовались;
- какой был результат;
- какие внешние сервисы были вызваны;
- какие AI-модели участвовали;
- какая была уверенность ответа;
- были ли откаты.

---

### 8.5 AI не считается абсолютно надёжным

AI-ответы в Num должны иметь специальный тип неопределённости.

AI-результат нельзя использовать как точный факт без проверки.

Пример:

```num
let risk: Uncertain<RiskLevel> = ai.assess_risk(profile)

approve_loan(risk)
```

Ошибка компиляции:

```text
Cannot pass Uncertain<RiskLevel> where RiskLevel is required.
Handle confidence or require human review.
```

---

## 9. Синтаксис языка

### 9.1 Общий стиль

Синтаксис должен быть читаемым, современным и близким к языкам Rust, Kotlin, Swift и TypeScript.

Основные требования:

- фигурные скобки для блоков;
- `let` для объявления переменных;
- `fn` для функций;
- `type` для структур данных;
- `enum` для перечислений;
- `match` для pattern matching;
- `workflow` для бизнес-процессов;
- `service` для API-сервисов;
- `policy` для правил доступа;
- `permission` для права доступа;
- `role` для роли пользователя;
- `transaction` для атомарных блоков;
- `action` для внешних действий;
- `rollback` для компенсирующих операций.

---

### 9.2 Переменные

```num
let name: Text = "Aidar"
let age: Int = 25
let amount: Money<KZT> = 15000 KZT
```

Изменяемые переменные:

```num
var counter: Int = 0
counter = counter + 1
```

По умолчанию переменные неизменяемые.

---

### 9.3 Типы

```num
type User {
    id: UserId
    name: Text
    email: Email private
    role: Role
}
```

---

### 9.4 Enum

```num
enum Role {
    Admin
    Manager
    Support
    User
}
```

---

### 9.5 Функции

```num
fn calculate_total(items: List<Item>) -> Money<KZT> {
    let total = items.sum(item => item.price)
    return total
}
```

---

### 9.6 Ошибки

Ошибки должны быть частью типа результата.

```num
fn find_user(id: UserId) -> Result<User, UserNotFound> {
    ...
}
```

Использование:

```num
let user = find_user(id)?
```

---

### 9.7 Pattern matching

```num
match payment.status {
    Paid => send_receipt(payment)
    Failed(reason) => notify_failure(reason)
    Pending => wait()
}
```

---

### 9.8 Nullable-значения

В языке не должно быть обычного `null`.

Вместо него используется `Option<T>`:

```num
let phone: Option<PhoneNumber> = user.phone
```

---

## 10. Система типов

### 10.1 Статическая типизация

Num должен иметь строгую статическую типизацию.

Ошибки типов должны выявляться на этапе компиляции.

---

### 10.2 Type inference

Компилятор должен уметь выводить типы:

```num
let name = "Aidar"
let count = 10
```

Но для публичных API, workflow, action и policy рекомендуется явная типизация.

---

### 10.3 Generic-типы

```num
type Page<T> {
    items: List<T>
    total: Int
    page: Int
}
```

---

### 10.4 Union-типы

```num
type SearchResult = User | Company | Document
```

---

### 10.5 Branded-типы

Для предотвращения смешивания одинаковых примитивов:

```num
type UserId = Brand<Text, "UserId">
type OrderId = Brand<Text, "OrderId">
```

Нельзя случайно передать `OrderId` вместо `UserId`.

---

### 10.6 Единицы измерения

Язык должен поддерживать безопасную работу с единицами:

```num
let distance: Distance<Kilometer> = 10 km
let time: Duration<Hour> = 2 h
let speed = distance / time
```

---

### 10.7 Деньги и валюты

Деньги должны быть отдельным типом:

```num
let price: Money<KZT> = 1000 KZT
let usd: Money<USD> = 10 USD
```

Нельзя сложить разные валюты без явной конвертации.

```num
price + usd
```

Ошибка:

```text
Cannot add Money<KZT> and Money<USD> without exchange rate.
```

---

## 11. Типы происхождения данных

### 11.1 Источник данных

Каждое значение может иметь источник:

```num
let text: Text from UserInput
let record: User from Database
let response: Text from ExternalApi
let answer: Text from AI
```

Базовые источники:

- `UserInput`;
- `Database`;
- `ExternalApi`;
- `AI`;
- `FileUpload`;
- `System`;
- `Config`;
- `SecretStore`;
- `InternalService`;
- `PublicData`;
- `ManualReview`.

---

### 11.2 Доверие к данным

Данные могут иметь уровень доверия:

```num
let profile: UserProfile trusted
let message: Text untrusted
let result: Text verified
```

Уровни:

- `untrusted`;
- `partially_trusted`;
- `trusted`;
- `verified`;
- `system_verified`.

---

### 11.3 Приватность данных

```num
let email: Email private
let password: Secret
let public_name: Text public
```

Уровни приватности:

- `public`;
- `internal`;
- `private`;
- `sensitive`;
- `secret`;
- `regulated`.

Примеры:

```num
let iin: Text regulated
let token: Text secret
let diagnosis: Text sensitive
```

---

### 11.4 Политики использования данных

Разработчик должен иметь возможность описывать, какие данные куда можно отправлять.

```num
policy DataSharing {
    allow private UserInput -> InternalService
    deny private UserInput -> ExternalApi
    allow public PublicData -> ExternalApi
}
```

---

## 12. Система прав доступа

### 12.1 Permission-типы

```num
permission ViewBilling
permission IssueRefund
permission DeleteUser
permission ExportData
```

---

### 12.2 Функции с правами

```num
fn issue_refund(payment: Payment) requires Permission.IssueRefund {
    ...
}
```

---

### 12.3 Роли

```num
role SupportAgent {
    allow ViewBilling
}

role FinanceManager {
    allow ViewBilling
    allow IssueRefund
}
```

---

### 12.4 Проверка прав на этапе компиляции и runtime

Если права известны статически, компилятор должен проверять их заранее.

Если права зависят от пользователя, runtime должен проверять их во время выполнения.

```num
require Permission.IssueRefund for current_user
```

---

### 12.5 Контекст безопасности

Каждый request, workflow и action должен иметь security context:

```num
context SecurityContext {
    actor: User
    roles: List<Role>
    permissions: Set<Permission>
    tenant: TenantId
}
```

---

## 13. Работа с AI

### 13.1 AI как встроенный модуль языка

AI-вызовы должны быть первоклассной частью языка.

```num
let summary = ai.summarize(document)
```

---

### 13.2 Uncertain-типы

Любой AI-ответ по умолчанию возвращает `Uncertain<T>`.

```num
let category: Uncertain<Category> = ai.classify(text)
```

Структура:

```num
type Uncertain<T> {
    value: T
    confidence: Float
    source: AIModel
    explanation: Option<Text>
    evidence: List<Evidence>
}
```

---

### 13.3 Обязательная обработка уверенности

Нельзя использовать `Uncertain<T>` как обычный `T`.

```num
let category: Category = ai.classify(text)
```

Ошибка:

```text
AI result is uncertain. Handle confidence explicitly.
```

Правильно:

```num
let result = ai.classify(text)

if result.confidence >= 0.9 {
    process(result.value)
} else {
    require_human_review(result)
}
```

---

### 13.4 AI-policy

```num
policy AIUsage {
    allow model "small" for public data
    allow model "enterprise-secure" for private data
    deny external_model for regulated data
}
```

---

### 13.5 Prompt injection protection

Язык должен иметь встроенные механизмы:

- разделение instruction и data;
- запрет выполнения команд из пользовательского контента;
- маркировку недоверенного текста;
- автоматическое экранирование контента;
- проверку tool calls.

Пример:

```num
let user_text: Text from UserInput untrusted

ai.ask(
    instruction: "Summarize the text",
    data: user_text
)
```

Пользовательский текст не должен иметь возможность изменить instruction.

---

### 13.6 AI tool calling

AI может предлагать действия, но не должен выполнять их без проверки политики.

```num
let proposal = ai.plan_actions(request)

approve_actions(proposal) with policy ActionSafety
```

---

### 13.7 Human-in-the-loop

Для критичных действий язык должен поддерживать ручное подтверждение.

```num
require_human_approval(
    action: issue_refund(payment),
    reason: "AI confidence below threshold"
)
```

---

## 14. Workflow-система

### 14.1 Workflow как часть языка

```num
workflow onboard_customer(customer: Customer) {
    verify_identity(customer)
    create_account(customer)
    send_welcome_email(customer)
}
```

---

### 14.2 Состояния workflow

Workflow должен иметь состояние:

- created;
- running;
- waiting;
- failed;
- compensated;
- completed;
- cancelled.

---

### 14.3 Долгоживущие процессы

Workflow может длиться минуты, дни или недели.

Язык должен поддерживать:

- паузы;
- ожидание внешнего события;
- retry;
- timeout;
- schedule;
- resume;
- cancellation;
- compensation.

```num
workflow contract_approval(contract: Contract) {
    legal.review(contract)

    wait until finance.approved(contract.id) timeout 7 days

    if timeout {
        notify_owner(contract)
        cancel workflow
    }

    ceo.approve(contract)
}
```

---

### 14.4 Идемпотентность

Действия в workflow должны поддерживать idempotency keys.

```num
action send_email(to: Email, body: Text)
    idempotent by hash(to, body)
```

---

### 14.5 Компенсации

```num
action reserve_stock(items: List<Item>)
    rollback release_stock(items)

action charge_card(card: Card, amount: Money<KZT>)
    rollback refund_card(card, amount)
```

---

## 15. Action-система

### 15.1 Action

`action` — это операция, которая влияет на внешний мир.

Примеры:

- отправить email;
- списать деньги;
- создать запись в CRM;
- вызвать внешний API;
- изменить статус договора;
- отправить SMS;
- создать файл;
- отправить push.

```num
action send_email(to: Email, body: Text) {
    smtp.send(to, body)
}
```

---

### 15.2 Action metadata

Каждый action должен иметь:

- имя;
- входные данные;
- выходные данные;
- уровень риска;
- права;
- audit;
- rollback;
- retry policy;
- timeout;
- cost;
- idempotency key.

Пример:

```num
action issue_refund(payment: Payment, amount: Money<KZT>)
    requires Permission.IssueRefund
    risk high
    cost max 0 KZT
    timeout 10s
    rollback reverse_refund(payment, amount)
{
    payment_gateway.refund(payment.id, amount)
}
```

---

## 16. Транзакции

### 16.1 Локальные транзакции

```num
transaction {
    db.users.insert(user)
    db.audit.insert(event)
}
```

---

### 16.2 Распределённые транзакции

Для внешних сервисов используется saga-подход с компенсациями.

```num
transaction saga {
    reserve_stock(order.items)
    charge_customer(order.customer, order.total)
    create_shipping(order)
}
```

Если `create_shipping` падает, выполняются компенсации предыдущих шагов.

---

### 16.3 Политика rollback

```num
transaction saga rollback_order reverse {
    step reserve_stock rollback release_stock
    step charge_customer rollback refund_customer
    step create_shipping rollback cancel_shipping
}
```

---

## 17. Аудит

### 17.1 Встроенный audit trail

Язык должен иметь встроенный audit API.

```num
audit("refund_issued", {
    payment_id: payment.id,
    amount: amount,
    actor: current_user.id
})
```

---

### 17.2 Автоматический аудит

Для критичных action audit должен быть обязательным.

```num
action delete_user(user: User)
    risk critical
{
    ...
}
```

Компилятор должен требовать audit.

---

### 17.3 Audit-схема

Каждая audit-запись должна содержать:

- event_id;
- timestamp;
- actor;
- tenant;
- action;
- input summary;
- data sources;
- permissions used;
- AI models used;
- confidence values;
- result;
- rollback status;
- correlation_id;
- request_id.

---

## 18. Стоимость и лимиты

### 18.1 Cost-aware programming

Num должен позволять задавать стоимость операций.

```num
@max_cost("0.05 USD")
fn summarize(document: Document) -> Summary {
    ai.model("large").summarize(document)
}
```

---

### 18.2 Бюджет workflow

```num
workflow analyze_documents(docs: List<Document>)
    budget max "5 USD"
{
    for doc in docs {
        ai.extract(doc)
    }
}
```

---

### 18.3 Лимиты времени

```num
@timeout("3s")
fn fetch_user(id: UserId) -> User {
    api.get_user(id)
}
```

---

### 18.4 Rate limits

```num
connector crm {
    rate_limit 100 requests per minute
}
```

---

## 19. Конкурентность и асинхронность

### 19.1 Async/await

```num
let user = await users.fetch(id)
```

---

### 19.2 Structured concurrency

Все async-задачи должны иметь владельца и не должны “теряться”.

```num
scope {
    let a = async fetch_profile(id)
    let b = async fetch_orders(id)

    let profile = await a
    let orders = await b
}
```

---

### 19.3 Actor model

Для stateful-сервисов:

```num
actor CartActor {
    state cart: Cart

    fn add_item(item: Item) {
        cart.items.push(item)
    }
}
```

---

### 19.4 Очереди и события

```num
event PaymentReceived {
    payment_id: PaymentId
    amount: Money<KZT>
}

on PaymentReceived as event {
    process_payment(event)
}
```

---

## 20. Модули и пакеты

### 20.1 Модули

```num
module billing.refunds
```

Импорт:

```num
use billing.payments.Payment
use security.Permission
```

---

### 20.2 Пакетный менеджер

Пакетный менеджер должен называться **num pkg** или **numctl**.

Функции:

- установка пакетов;
- публикация пакетов;
- lock-файл;
- проверка зависимостей;
- security audit;
- policy audit;
- dependency provenance;
- SBOM generation.

Пример:

```bash
num pkg add http
num pkg add postgres
num pkg audit
```

---

### 20.3 Lock-файл

Файл:

```text
num.lock
```

Должен фиксировать:

- версии пакетов;
- хэши;
- источники;
- лицензии;
- security metadata;
- policy requirements.

---

## 21. Стандартная библиотека

### 21.1 Базовые типы

- `Text`;
- `Int`;
- `Float`;
- `Decimal`;
- `Bool`;
- `Date`;
- `DateTime`;
- `Duration`;
- `Uuid`;
- `Email`;
- `PhoneNumber`;
- `Url`;
- `Json`;
- `Xml`;
- `Bytes`.

---

### 21.2 Коллекции

- `List<T>`;
- `Map<K, V>`;
- `Set<T>`;
- `Queue<T>`;
- `Stack<T>`;
- `Stream<T>`.

---

### 21.3 Result и Option

```num
Result<T, E>
Option<T>
```

---

### 21.4 Деньги

```num
Money<KZT>
Money<USD>
Money<EUR>
```

---

### 21.5 Документы

- `Document`;
- `Pdf`;
- `Docx`;
- `Image`;
- `Spreadsheet`;
- `OcrResult`;
- `ExtractedData`.

---

### 21.6 AI

- `ai.classify`;
- `ai.extract`;
- `ai.summarize`;
- `ai.embed`;
- `ai.rerank`;
- `ai.generate`;
- `ai.plan`;
- `ai.verify`.

---

### 21.7 HTTP

```num
http.get(url)
http.post(url, body)
```

---

### 21.8 Database

Поддержка:

- PostgreSQL;
- MySQL;
- SQLite;
- Redis;
- MongoDB;
- vector databases.

---

### 21.9 Security

- permissions;
- roles;
- policies;
- encryption;
- hashing;
- secrets;
- JWT;
- OAuth;
- session management.

---

## 22. Connectors

### 22.1 Назначение

Connectors нужны для безопасного взаимодействия с внешними сервисами.

Примеры:

- Gmail;
- Google Calendar;
- Slack;
- Telegram;
- Notion;
- Salesforce;
- HubSpot;
- Stripe;
- Kaspi;
- Freedom Bank;
- PostgreSQL;
- S3;
- OpenAI-compatible AI APIs.

---

### 22.2 Connector schema

```num
connector stripe {
    auth secret STRIPE_API_KEY
    base_url "https://api.stripe.com"

    rate_limit 100 requests per minute

    policy {
        allow MoneyOperation only with Permission.ManagePayments
    }
}
```

---

### 22.3 Безопасность connector

Connector должен указывать:

- какие данные принимает;
- какие данные возвращает;
- какие privacy-level разрешены;
- какие permissions нужны;
- какой риск операции;
- какие audit-события создаются;
- есть ли rollback.

---

## 23. Компилятор

### 23.1 Название

Компилятор: **numc**

---

### 23.2 Основные задачи компилятора

Компилятор должен выполнять:

- лексический анализ;
- синтаксический анализ;
- построение AST;
- проверку типов;
- проверку provenance;
- проверку privacy rules;
- проверку permissions;
- проверку AI uncertainty;
- проверку rollback;
- проверку audit;
- проверку cost limits;
- генерацию IR;
- оптимизацию;
- генерацию целевого кода.

---

### 23.3 Intermediate Representation

Num должен иметь собственный IR.

IR должен хранить не только вычисления, но и:

- provenance metadata;
- privacy labels;
- permission requirements;
- cost annotations;
- rollback graph;
- audit requirements;
- uncertainty markers.

---

### 23.4 Целевые платформы

Полная версия языка должна поддерживать несколько backend-целей:

1. Native binary через LLVM.
2. WebAssembly.
3. JavaScript/TypeScript interop.
4. JVM interop.
5. Container runtime для cloud deployment.

---

### 23.5 Ошибки компилятора

Ошибки должны быть понятными и объяснять не только “что сломалось”, но и “почему это опасно”.

Пример:

```text
Error C2031: Private data leak

Value:
  ticket.email: Email from UserInput private

Attempted operation:
  send to ExternalApi "analytics"

Reason:
  DataSharing policy denies private UserInput data to ExternalApi.

Fix:
  - anonymize the value
  - add explicit policy exception
  - send only hashed value
```

---

## 24. Runtime

### 24.1 Назначение runtime

Runtime отвечает за:

- выполнение workflow;
- управление состоянием;
- retry;
- timeout;
- rollback;
- audit;
- permissions;
- secrets;
- connector calls;
- cost tracking;
- observability;
- distributed execution.

---

### 24.2 Workflow engine

Runtime должен иметь встроенный durable workflow engine.

Функции:

- сохранение состояния workflow;
- восстановление после падения;
- replay;
- deterministic execution;
- scheduling;
- human approval;
- event waiting;
- compensation;
- cancellation.

---

### 24.3 Observability

Runtime должен поддерживать:

- logs;
- metrics;
- traces;
- audit events;
- cost metrics;
- AI call metrics;
- connector latency;
- retry count;
- rollback count.

---

### 24.4 Secrets

Секреты не должны попадать в обычные переменные.

```num
let api_key: Secret = secrets.get("OPENAI_API_KEY")
```

Секрет нельзя логировать:

```num
log(api_key)
```

Ошибка компиляции:

```text
Cannot log Secret value.
```

---

## 25. Безопасность

### 25.1 Основные требования

Num должен защищать от:

- accidental data leaks;
- prompt injection;
- insecure deserialization;
- missing authorization;
- unsafe logging;
- secret exposure;
- SSRF;
- command injection;
- unsafe external API calls;
- privilege escalation;
- tenant data leakage;
- unsafe AI tool execution.

---

### 25.2 Tenant isolation

Для SaaS-систем язык должен поддерживать tenant-aware типы.

```num
type CustomerData tenant TenantId {
    ...
}
```

Нельзя случайно смешать данные разных tenant.

---

### 25.3 Политики безопасности

```num
policy TenantIsolation {
    deny data from Tenant<A> -> Tenant<B>
}
```

---

### 25.4 Sanitization

```num
let clean = sanitize.html(user_input)
```

Компилятор должен требовать sanitization для опасных контекстов:

- HTML;
- SQL;
- shell;
- prompt;
- URL;
- logs.

---

## 26. Инструменты разработчика

### 26.1 CLI

Команда: `num`

Основные команды:

```bash
num new project-name
num build
num run
num test
num fmt
num lint
num check
num deploy
num pkg add package
num pkg audit
num workflow inspect
num policy check
num cost analyze
```

---

### 26.2 Formatter

Единый форматтер:

```bash
num fmt
```

---

### 26.3 Linter

Linter должен проверять:

- стиль;
- небезопасные паттерны;
- неиспользуемые permissions;
- слабые policies;
- дорогие AI-вызовы;
- отсутствие audit;
- слабую обработку ошибок.

---

### 26.4 Language Server

Должен быть LSP-сервер для:

- VS Code;
- JetBrains IDE;
- Vim/Neovim;
- Zed;
- Cursor.

Функции:

- автодополнение;
- hover-информация;
- go to definition;
- rename;
- refactor;
- diagnostics;
- inline cost estimates;
- inline privacy warnings;
- AI uncertainty hints;
- policy explanation.

---

### 26.5 Debugger

Debugger должен поддерживать:

- step execution;
- breakpoints;
- watch variables;
- inspection of provenance;
- inspection of permissions;
- workflow state;
- rollback graph;
- AI calls;
- connector calls.

---

## 27. Тестирование

### 27.1 Unit tests

```num
test "calculate total" {
    let total = calculate_total(items)
    assert total == 1500 KZT
}
```

---

### 27.2 Policy tests

```num
test policy "private data cannot go to external API" {
    expect_deny {
        external.analytics.send(user.email)
    }
}
```

---

### 27.3 Workflow tests

```num
test workflow "refund rollback" {
    simulate failure at send_notification

    run process_refund(payment)

    assert payment.refunded == false
    assert audit.contains("rollback_completed")
}
```

---

### 27.4 AI tests

```num
test ai "low confidence requires review" {
    mock ai.classify returns confidence 0.4

    run handle_ticket(ticket)

    assert human_review.created == true
}
```

---

## 28. Документация

Документация должна включать:

- официальный сайт;
- tutorial;
- language reference;
- standard library reference;
- compiler guide;
- runtime guide;
- workflow guide;
- AI safety guide;
- security model;
- examples;
- migration guides;
- cookbook;
- best practices;
- FAQ.

---

## 29. Экосистема

Полная экосистема Num должна включать:

- компилятор;
- runtime;
- package manager;
- стандартную библиотеку;
- LSP;
- VS Code extension;
- JetBrains plugin;
- formatter;
- linter;
- debugger;
- workflow dashboard;
- audit dashboard;
- policy editor;
- connector marketplace;
- cloud deployment tool;
- documentation site;
- examples repository.

---

## 30. Dashboard

### 30.1 Workflow dashboard

Должен показывать:

- активные workflow;
- failed workflow;
- waiting workflow;
- completed workflow;
- rollback events;
- human approval tasks;
- retry attempts;
- duration;
- cost.

---

### 30.2 Audit dashboard

Должен показывать:

- кто выполнил действие;
- когда;
- какие данные были использованы;
- какие permissions были применены;
- какие AI-модели участвовали;
- какие внешние API были вызваны;
- были ли ошибки;
- был ли rollback.

---

### 30.3 Cost dashboard

Должен показывать:

- стоимость AI-вызовов;
- стоимость API-вызовов;
- стоимость workflow;
- стоимость по пользователям;
- стоимость по tenant;
- превышение лимитов;
- прогноз затрат.

---

## 31. Deployment

### 31.1 Целевые среды

Num должен поддерживать:

- local development;
- Docker;
- Kubernetes;
- bare metal;
- serverless;
- edge runtime;
- WebAssembly runtime.

---

### 31.2 Конфигурация deployment

```num
deploy production {
    target kubernetes
    replicas 3
    secrets from vault
    observability enabled
    audit sink postgres
}
```

---

### 31.3 CI/CD

Поддержка:

- GitHub Actions;
- GitLab CI/CD;
- Jenkins;
- Docker registry;
- Kubernetes deployment;
- policy check before deploy;
- cost analysis before deploy;
- security audit before deploy.

---

## 32. Interop

### 32.1 Вызов внешнего кода

Num должен уметь вызывать:

- C;
- Rust;
- Python;
- JavaScript;
- JVM languages;
- WebAssembly modules.

---

### 32.2 Импорт OpenAPI

```bash
num import openapi ./crm.yaml
```

Должны генерироваться:

- typed client;
- connector;
- permission requirements;
- data policy placeholders.

---

### 32.3 Импорт database schema

```bash
num import db postgres://...
```

Должны генерироваться типы таблиц и безопасные query API.

---

## 33. Performance

### 33.1 Требования

Язык должен быть достаточно быстрым для backend-сервисов.

Цели:

- native performance для вычислительных задач;
- минимальный overhead для обычного кода;
- контролируемый overhead для audit/provenance;
- async runtime для высоконагруженных сервисов;
- оптимизация workflow execution;
- zero-cost abstractions там, где возможно.

---

### 33.2 Оптимизации

Компилятор должен поддерживать:

- dead code elimination;
- inlining;
- escape analysis;
- effect analysis;
- policy precomputation;
- provenance compression;
- cost graph optimization.

---

## 34. Версионирование языка

Num должен использовать semantic versioning.

Пример:

```text
Num 1.0.0
Num 1.1.0
Num 2.0.0
```

---

## 35. Совместимость

Нужно поддерживать:

- backward compatibility для minor versions;
- migration tools для major versions;
- deprecation warnings;
- automatic code fix suggestions.

---

## 36. Лицензирование

Рекомендуемая модель:

- язык и компилятор — open source;
- стандартная библиотека — open source;
- core runtime — open source;
- enterprise dashboard — commercial;
- managed cloud — commercial;
- premium connectors — commercial;
- enterprise policy packs — commercial.

---

## 37. Монетизация

Возможные источники дохода:

1. Managed Num Cloud.
2. Enterprise workflow runtime.
3. Audit dashboard.
4. Premium connectors.
5. Compliance packs.
6. Security policy packs.
7. Enterprise support.
8. Private package registry.
9. On-premise лицензия.
10. Consulting and migration.

---

## 38. Отличие от существующих языков

### 38.1 Python

Python удобный и быстрый для разработки, но не защищает от:

- утечки данных;
- игнорирования AI uncertainty;
- отсутствия прав доступа;
- отсутствия rollback;
- неправильного логирования секретов.

Num решает это на уровне языка.

---

### 38.2 TypeScript

TypeScript хорош для frontend/backend, но большинство security/business-инвариантов реализуются вручную.

Num делает эти инварианты частью типа и политики.

---

### 38.3 Rust

Rust отлично решает безопасность памяти, но не решает автоматически:

- бизнес-права;
- приватность данных;
- AI uncertainty;
- workflow rollback;
- audit;
- cost limits.

Num фокусируется не на memory safety как главной идее, а на operational safety.

---

### 38.4 Go

Go прост для backend, но многие системные гарантии остаются на уровне conventions.

Num делает критичные гарантии обязательными.

---

### 38.5 Java/Kotlin/C#

Эти языки сильны в enterprise, но права, audit, workflow и AI safety обычно выносятся во фреймворки.

Num встраивает это в сам язык.

---

## 39. Основные сущности языка

| Сущность | Назначение |
|---|---|
| `fn` | обычная функция |
| `type` | структура данных |
| `enum` | перечисление |
| `workflow` | бизнес-процесс |
| `action` | внешнее действие |
| `policy` | правило безопасности |
| `permission` | право доступа |
| `role` | роль пользователя |
| `connector` | внешний сервис |
| `transaction` | атомарный или saga-блок |
| `audit` | журнал действия |
| `Uncertain<T>` | неопределённый результат |
| `Secret` | секретное значение |
| `Money<C>` | деньги в валюте C |

---

## 40. Пример полноценного модуля

```num
module billing.refunds

permission ViewBilling
permission IssueRefund

type RefundRequest {
    payment_id: PaymentId
    reason: Text from UserInput private
    amount: Money<KZT>
}

action issue_refund(payment: Payment, amount: Money<KZT>)
    requires Permission.IssueRefund
    risk high
    timeout 10s
    rollback reverse_refund(payment, amount)
{
    payment_gateway.refund(payment.id, amount)

    audit("refund_issued", {
        payment_id: payment.id,
        amount: amount,
        actor: current_user.id
    })
}

workflow process_refund(request: RefundRequest) {
    require Permission.ViewBilling for current_user

    let payment = payments.find(request.payment_id)?

    if request.amount > payment.amount {
        reject("Refund amount is greater than payment amount")
        return
    }

    let risk: Uncertain<RiskLevel> = ai.assess_refund_risk(request)

    if risk.confidence < 0.85 {
        require_human_approval(
            action: "issue_refund",
            reason: "Low AI confidence"
        )
    }

    require Permission.IssueRefund for current_user

    transaction saga {
        issue_refund(payment, request.amount)
        notify_customer(payment.customer.email)
        audit("refund_workflow_completed")
    }
}
```

---

## 41. Нефункциональные требования

### 41.1 Надёжность

- runtime должен восстанавливаться после падений;
- workflow не должны терять состояние;
- audit-записи должны быть устойчивыми;
- внешние действия должны быть идемпотентными там, где возможно.

---

### 41.2 Безопасность

- секреты нельзя логировать;
- приватные данные нельзя отправлять без policy;
- tenant data isolation обязателен;
- AI tool calls должны проверяться;
- permissions должны быть обязательными для критичных действий.

---

### 41.3 Масштабируемость

- поддержка горизонтального масштабирования;
- distributed workflow execution;
- очереди;
- worker pools;
- cloud-native deployment.

---

### 41.4 Расширяемость

- plugin system;
- custom connectors;
- custom policies;
- custom AI providers;
- custom runtime backends;
- custom compiler diagnostics.

---

### 41.5 Developer Experience

- понятные ошибки;
- быстрый feedback loop;
- качественный LSP;
- удобный CLI;
- автогенерация connectors;
- понятная документация;
- встроенные примеры.

---

## 42. Архитектура проекта

### 42.1 Репозитории

Рекомендуемая структура:

```text
num/
  compiler/
  runtime/
  stdlib/
  cli/
  lsp/
  formatter/
  linter/
  dashboard/
  connectors/
  docs/
  examples/
  tests/
```

---

### 42.2 Compiler architecture

```text
Source Code
   ↓
Lexer
   ↓
Parser
   ↓
AST
   ↓
Type Checker
   ↓
Effect Checker
   ↓
Policy Checker
   ↓
Provenance Checker
   ↓
IR
   ↓
Optimizer
   ↓
Code Generator
   ↓
Target Output
```

---

### 42.3 Runtime architecture

```text
Runtime
  ├── Workflow Engine
  ├── Action Executor
  ├── Policy Engine
  ├── Audit Engine
  ├── AI Gateway
  ├── Connector Gateway
  ├── Secrets Manager
  ├── Cost Tracker
  ├── Scheduler
  ├── State Store
  └── Observability Layer
```

---

## 43. Файловая структура проекта на Num

```text
my-app/
  num.toml
  num.lock
  src/
    main.num
    billing/
      refunds.num
      payments.num
    security/
      roles.num
      policies.num
    workflows/
      onboarding.num
  tests/
    billing_test.num
    policy_test.num
  connectors/
    stripe.num
    crm.num
  deploy/
    production.num
```

---

## 44. Конфигурационный файл

Файл: `num.toml`

```toml
[project]
name = "billing-service"
version = "1.0.0"

[runtime]
workflow_store = "postgres"
audit_store = "postgres"
secrets = "vault"

[ai]
default_provider = "enterprise-secure"
max_monthly_cost = "1000 USD"

[security]
policy_mode = "strict"
tenant_isolation = true

[deploy]
target = "kubernetes"
```

---

## 45. Требования к полной версии языка

Полная версия Num должна включать:

- компилятор;
- статическую типизацию;
- стандартную библиотеку;
- workflow engine;
- action system;
- policy system;
- permission system;
- audit system;
- AI module;
- uncertainty types;
- provenance types;
- privacy labels;
- connector system;
- package manager;
- CLI;
- LSP;
- formatter;
- linter;
- debugger;
- dashboard;
- deployment tooling;
- документацию;
- примеры;
- test framework.

---

## 46. Риски проекта

### 46.1 Сложность языка

Риск: язык может стать слишком сложным.

Решение:

- простой базовый синтаксис;
- постепенное раскрытие возможностей;
- хорошие ошибки компилятора;
- качественная документация;
- presets для типовых сценариев.

---

### 46.2 Высокий порог входа

Риск: разработчики не захотят учить новый язык.

Решение:

- синтаксис похожий на популярные языки;
- interop с TypeScript/Python/Rust;
- генерация connectors;
- понятные примеры;
- фокус на реальных болях AI/backend-команд.

---

### 46.3 Производительность

Риск: provenance, audit и policy checks могут замедлить runtime.

Решение:

- compile-time checks;
- static policy analysis;
- оптимизация IR;
- отключаемые уровни трассировки;
- efficient metadata representation.

---

### 46.4 Конкуренция с фреймворками

Риск: команды предпочтут фреймворки поверх существующих языков.

Решение:

- дать гарантии, которые невозможно удобно получить библиотеками;
- сделать Num особенно сильным для AI/workflow/security-сценариев;
- обеспечить interop с существующей инфраструктурой.

---

## 47. Критерии успеха

Проект можно считать успешным, если:

- на Num можно написать production-ready backend-сервис;
- компилятор предотвращает реальные классы ошибок;
- AI-ответы нельзя случайно использовать без проверки;
- приватные данные нельзя случайно отправить во внешний API;
- workflow переживают падение runtime;
- audit trail создаётся автоматически для критичных действий;
- permissions проверяются системно;
- разработчик получает понятные ошибки;
- язык имеет полноценную документацию и инструменты;
- есть рабочая экосистема пакетов и connectors.

---

## 48. Финальный образ продукта

Num должен стать не просто новым языком программирования, а платформой для создания безопасных, проверяемых и ответственных программ.

Итоговый продукт включает:

- язык;
- компилятор;
- runtime;
- workflow engine;
- AI safety layer;
- policy engine;
- audit system;
- connector ecosystem;
- developer tools;
- dashboard;
- cloud/on-prem deployment.

Главная ценность:

> Num позволяет писать программы, которые не просто выполняют команды, а понимают ограничения, последствия и ответственность каждого действия.
