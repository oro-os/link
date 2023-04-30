/* USER CODE BEGIN Header */
/**
  ******************************************************************************
  * @file           : main.h
  * @brief          : Header for main.c file.
  *                   This file contains the common defines of the application.
  ******************************************************************************
  * @attention
  *
  * Copyright (c) 2023 STMicroelectronics.
  * All rights reserved.
  *
  * This software is licensed under terms that can be found in the LICENSE file
  * in the root directory of this software component.
  * If no LICENSE file comes with this software, it is provided AS-IS.
  *
  ******************************************************************************
  */
/* USER CODE END Header */

/* Define to prevent recursive inclusion -------------------------------------*/
#ifndef __MAIN_H
#define __MAIN_H

#ifdef __cplusplus
extern "C" {
#endif

/* Includes ------------------------------------------------------------------*/
#include "stm32f4xx_hal.h"

/* Private includes ----------------------------------------------------------*/
/* USER CODE BEGIN Includes */

/* USER CODE END Includes */

/* Exported types ------------------------------------------------------------*/
/* USER CODE BEGIN ET */

/* USER CODE END ET */

/* Exported constants --------------------------------------------------------*/
/* USER CODE BEGIN EC */

/* USER CODE END EC */

/* Exported macro ------------------------------------------------------------*/
/* USER CODE BEGIN EM */

/* USER CODE END EM */

/* Exported functions prototypes ---------------------------------------------*/
void Error_Handler(void);

/* USER CODE BEGIN EFP */

/* USER CODE END EFP */

/* Private defines -----------------------------------------------------------*/
#define ETH2_RST__Pin GPIO_PIN_13
#define ETH2_RST__GPIO_Port GPIOC
#define ETH1_RST__Pin GPIO_PIN_0
#define ETH1_RST__GPIO_Port GPIOB
#define ETH1_INT__Pin GPIO_PIN_1
#define ETH1_INT__GPIO_Port GPIOB
#define ETH1_INT__EXTI_IRQn EXTI1_IRQn
#define DBG_LED_Pin GPIO_PIN_12
#define DBG_LED_GPIO_Port GPIOE
#define PSU_ON_Pin GPIO_PIN_13
#define PSU_ON_GPIO_Port GPIOB
#define PSU_OK_Pin GPIO_PIN_14
#define PSU_OK_GPIO_Port GPIOB
#define SYS_RESET_Pin GPIO_PIN_15
#define SYS_RESET_GPIO_Port GPIOB
#define SYS_POWER_Pin GPIO_PIN_8
#define SYS_POWER_GPIO_Port GPIOD
#define OLED_RST__Pin GPIO_PIN_5
#define OLED_RST__GPIO_Port GPIOD
#define ETH2_INT__Pin GPIO_PIN_9
#define ETH2_INT__GPIO_Port GPIOB
#define ETH2_INT__EXTI_IRQn EXTI9_5_IRQn

/* USER CODE BEGIN Private defines */

/* USER CODE END Private defines */

#ifdef __cplusplus
}
#endif

#endif /* __MAIN_H */
