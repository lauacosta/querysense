document.addEventListener("DOMContentLoaded", function () {
	const content = document.querySelector(".content");
	const itemsPerPage = 10;
	let currentPage = 0;
	const items = Array.from(content.getElementsByTagName("tr")).slice(1); // Skip table header
	const totalPages = Math.ceil(items.length / itemsPerPage);

	function showPage(page) {
		const startIndex = page * itemsPerPage;
		const endIndex = startIndex + itemsPerPage;

		items.forEach((item, index) => {
			item.style.display =
				index >= startIndex && index < endIndex ? "" : "none";
		});

		updatePaginationInfo();
	}

	function createPaginationControls() {
		const paginationContainer = document.querySelector(".pagination");

		const prevButton = document.createElement("button");
		prevButton.textContent = "<";
		prevButton.addEventListener("click", () => {
			if (currentPage > 0) {
				currentPage--;
				showPage(currentPage);
			}
		});
		paginationContainer.appendChild(prevButton);

		const pageInfo = document.createElement("span");
		pageInfo.classList.add("page-info");
		paginationContainer.appendChild(pageInfo);

		const nextButton = document.createElement("button");
		nextButton.textContent = ">";
		nextButton.addEventListener("click", () => {
			if (currentPage < totalPages - 1) {
				currentPage++;
				showPage(currentPage);
			}
		});
		paginationContainer.appendChild(nextButton);

		updatePaginationInfo();
	}

	function updatePaginationInfo() {
		const pageInfo = document.querySelector(".page-info");
		pageInfo.textContent = `Page ${currentPage + 1} of ${totalPages}`;
	}

	createPaginationControls();
	showPage(currentPage);
});
