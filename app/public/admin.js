(() => {
    const markdownTextarea = () => document.querySelector('textarea[name="body_markdown"]');

    const appendMarkdown = (textarea, markdown) => {
        const start = textarea.selectionStart ?? textarea.value.length;
        const end = textarea.selectionEnd ?? textarea.value.length;
        const before = textarea.value.slice(0, start);
        const after = textarea.value.slice(end);
        const prefix = before.length === 0 || before.endsWith("\n") ? "" : "\n\n";
        const suffix = after.length === 0 || after.startsWith("\n") ? "" : "\n\n";
        const insertion = `${prefix}${markdown}${suffix}`;
        const cursor = before.length + insertion.length;

        textarea.value = `${before}${insertion}${after}`;
        textarea.focus();
        textarea.setSelectionRange(cursor, cursor);
        textarea.dispatchEvent(new Event("input", { bubbles: true }));
    };

    const copyText = async (value) => {
        if (navigator.clipboard && window.isSecureContext) {
            await navigator.clipboard.writeText(value);
            return;
        }

        const textarea = document.createElement("textarea");
        textarea.value = value;
        textarea.setAttribute("readonly", "");
        textarea.style.position = "fixed";
        textarea.style.left = "-9999px";
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        textarea.remove();
    };

    const temporarilyRename = (button, label) => {
        const original = button.textContent;
        button.textContent = label;
        window.setTimeout(() => {
            button.textContent = original;
        }, 1200);
    };

    document.addEventListener("click", async (event) => {
        const insertButton = event.target.closest(".insert-markdown-image[data-markdown]");
        if (insertButton) {
            const textarea = markdownTextarea();
            if (textarea) {
                appendMarkdown(textarea, insertButton.dataset.markdown);
                temporarilyRename(insertButton, "Inserted");
            }
            return;
        }

        const copyButton = event.target.closest(".copy-value[data-copy]");
        if (copyButton) {
            await copyText(copyButton.dataset.copy);
            temporarilyRename(copyButton, "Copied");
        }
    });
})();
