"""Operator checklist shared by KDE and GNOME VM stands."""

MANUAL_CHECKS = (
    ("launch-tray", "Запусти Tecloid из меню. Проверь окно, трей, закрытие и повторное открытие окна."),
    ("mapper", "Создай тестовый keymap: A → B и Ctrl+A. Проверь в редакторе, выключи и включи mapper."),
    ("layers-macros", "Проверь tap/hold, временный слой и макрос с паузой. После отпускания слой выключается."),
    ("text-popups", "Проверь текст Привет 🙂, Emoji и Quick actions, Escape и возврат фокуса в редактор. Разрешение портала — вручную."),
    ("settings", "Измени назначение, перезапусти приложение и проверь сохранение. Проверь темы, язык и маленькое окно."),
    ("autostart", "Включи автозапуск и перезагрузи VM: трей и mapper работают. Отключи и перезагрузи: приложение не запущено."),
    ("recovery", "Проверь блокировку/разблокировку, сон если доступен, повторный запуск и выход без залипших модификаторов."),
    ("removal", "Удали пакет: приложение удалено, пользовательские настройки сохранены."),
)


def manual_acceptance(report, ask=input, checks=MANUAL_CHECKS):
    """Only the operator supplies evidence; never inject guest input here."""
    report.require("manual-" + name for name, _ in checks)
    print("\nVM готова. Автоматический ввод отключён. Проверяй в окне QEMU.")
    print("После каждого пункта в этом терминале: pass / fail / skip и необязательное пояснение.")
    print("Ctrl+C завершает проверку с неполным результатом. Диск VM будет сохранён.")
    for name, instruction in checks:
        print("\n" + instruction)
        while True:
            answer = ask(f"{name} [pass/fail/skip]: ").strip().split(maxsplit=1)
            if answer and answer[0].lower() in ("pass", "fail", "skip"):
                status = {"pass": "passed", "fail": "FAILED", "skip": "skipped"}[answer[0].lower()]
                report.steps.append(("manual-" + name, status,
                                     "operator: " + (answer[1] if len(answer) > 1 else answer[0])))
                break

