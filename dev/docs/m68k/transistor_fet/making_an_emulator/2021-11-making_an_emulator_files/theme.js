
function changeHref(el, file) {
    el.setAttribute('href', el.getAttribute('href').replace(/assets.*$/, file));
}

function setTheme(theme, toggle) {
    if (theme == 'light') {
        changeHref(document.getElementById("pagetheme"), "assets/theme-light.css");
        changeHref(document.getElementById("codetheme"), "assets/code-default.css");
        if (toggle) toggle.innerText = 'dark';
    } else {
        changeHref(document.getElementById("pagetheme"), "assets/theme-dark.css");
        changeHref(document.getElementById("codetheme"), "assets/code-manni.css");
        if (toggle) toggle.innerText = 'light';
    }
}

var theme = window.localStorage.getItem('theme') || 'light';
setTheme(theme);

window.onload = function() {
    var toggle = document.getElementById("theme-toggle");
    setTheme(theme, toggle);

    toggle.onclick = function() {
        theme = (theme == 'light') ? 'dark' : 'light';
        window.localStorage.setItem('theme', theme);
        setTheme(theme, toggle);
    }
}

